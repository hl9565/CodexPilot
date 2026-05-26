# T13 · diagnostic_log 并行测试 flaky 调研

## Section 1：实测稳定性数据

本次调研在 `2026-05-26` 于仓库根目录执行以下两组命令，各跑 `10` 次：

- `cargo test -p codex-pilot-core --lib diagnostic_log`
- `cargo test -p codex-pilot-core --lib diagnostic_log -- --test-threads=1`

本轮 20 次采样里，`diagnostic_log` 目标测试均未复现失败；下面表格如实记录本轮结果。

### 默认并行（10 次）

| 次数 | 结果 | 失败 test |
| --- | --- | --- |
| 1 | PASS | - |
| 2 | PASS | - |
| 3 | PASS | - |
| 4 | PASS | - |
| 5 | PASS | - |
| 6 | PASS | - |
| 7 | PASS | - |
| 8 | PASS | - |
| 9 | PASS | - |
| 10 | PASS | - |

汇总：

| 组别 | pass | fail | 失败率 |
| --- | --- | --- | --- |
| 默认并行 | 10 | 0 | 0/10 = 0% |

### 单线程（10 次）

| 次数 | 结果 | 失败 test |
| --- | --- | --- |
| 1 | PASS | - |
| 2 | PASS | - |
| 3 | PASS | - |
| 4 | PASS | - |
| 5 | PASS | - |
| 6 | PASS | - |
| 7 | PASS | - |
| 8 | PASS | - |
| 9 | PASS | - |
| 10 | PASS | - |

汇总：

| 组别 | pass | fail | 失败率 |
| --- | --- | --- | --- |
| 单线程 | 10 | 0 | 0/10 = 0% |

结论：

- 本轮采样没有复现题述 flaky，不能用这 20 次结果直接证明问题已消失。
- 但“默认并行偶发失败、`--test-threads=1` 稳定”的历史现象，仍然与源码里的全局共享测试状态相符，值得继续按竞态方向分析。

## Section 2：失败时序分析

先看 `crates/codex-pilot-core/src/diagnostic_log.rs` 的关键实现：

- 测试覆盖路径依赖一个全局静态：`static TEST_LOG_PATH: Mutex<Option<PathBuf>>`
- `log_path()` 每次调用时都会临时读取这个全局值，然后立刻释放 `TEST_LOG_PATH` 的锁
- `append_rotates_large_log_file()` 和 `read_tail_spans_rotated_logs()` 通过 `test_log_guard()` 串行化
- `append_rotates_large_log_file()` 会先写一个 `20MB + 1` 的文件，再调用 `append()` 触发 `rotate_if_needed()`

### 假设 A：`test_log_guard()` 持锁时间不够，`rename` 后释放时另一个测试读到中间态

可能性判断：较低。

依据：

- 在同一文件系统内，`fs::rename(path, first)` 一般可视为单步目录项切换；调用返回成功后，`path` 和 `path.1` 不应再处于“半改名”状态。
- `append_rotates_large_log_file()` 在 `append()` 返回之后才断言 `rotated_path(&path, 1).exists()`，所以如果失败是单纯因为“当前这个 `rename` 还没落盘”，解释力不强。
- 真正的问题不太像“本次 rotate 自己没完成”，更像“完成后又被别的测试改走了”。

补充判断：

- `rename` 是否原子，不等于测试整体没有竞态。`log_path()` 只在函数入口读取一次全局路径；别的测试之后改写 `TEST_LOG_PATH`，会让后续调用落到不同目录。
- 所以 A 不是完全不可能，但不是最像根因的那个。

### 假设 B：多个测试共用 `TEST_LOG_PATH`，另一个测试的 rotate 或写入把 `path.1`“挤掉”

可能性判断：中等，但要分两层看。

第一层，`diagnostic_log.rs` 自己的 3 个测试之间：

- `append_rotates_large_log_file()` 与 `read_tail_spans_rotated_logs()` 都走 `test_log_guard()`，这两者之间不会并发。
- `redact_hides_secret_like_keys()` 不碰文件，也不改 `TEST_LOG_PATH`。
- 因此如果只看这 3 个测试本身，B 的解释力有限。

第二层，`codex-pilot-core` 其他测试：

- `crates/codex-pilot-core/src/routes.rs` 里还有一个 `#[tokio::test] diagnostics_report_uses_ok_shape` 会直接调用 `crate::diagnostic_log::set_test_log_path(root.join("diagnostic.log"))`
- 这个测试没有拿 `diagnostic_log.rs` 里的 `test_log_guard()`
- 它结束后还没有把 `TEST_LOG_PATH` 恢复回默认值

这意味着在 `cargo test --workspace` 或同一 test binary 默认并行时，存在下面的交叉时序：

1. `append_rotates_large_log_file()` 拿到 `test_log_guard()`，设置自己的 `TEST_LOG_PATH=A`
2. 另一个并行测试 `diagnostics_report_uses_ok_shape` 不受这把锁约束，改成 `TEST_LOG_PATH=B`
3. `append()` 内部调用 `log_path()` 时读到的是 `B`，不是预期的 `A`
4. 于是 rotate、append、断言检查不再围绕同一条文件路径展开，`rotated_path(&A, 1).exists()` 就可能失败

如果历史失败只在默认并行出现，而单线程稳定，那么这个“跨测试共享全局路径污染”比“同一个 rotate 流程自己没改成功”更符合现象。

### 假设 C：`metadata().len()` 与 `rename` 的时序问题

可能性判断：较低。

依据：

- `rotate_if_needed()` 先 `fs::metadata(path)`，如果长度达到阈值再做 rotate。
- 在 `append_rotates_large_log_file()` 里，测试自己刚写入的是一个明确大于 `MAX_LOG_BYTES` 的新文件，单测试视角下不太会出现“读到旧长度所以没 rotate”。
- 真要出现 C，更像是另一个测试把 `TEST_LOG_PATH` 改到了别的文件，导致 `metadata()` 读到的根本不是当前测试刚准备好的那一份大文件。那样的话，本质上仍然会回到“全局路径被并行测试污染”。

### 哪个更可能

如果只在 A / B / C 里选，我倾向于 **B 比 A、C 更可能**。

更准确地说，本次读源码后最像根因的版本是：

- 不是 `rename` 原子性本身有问题
- 也不是 `metadata().len()` 单独失真
- 而是 **`TEST_LOG_PATH` 是进程内共享全局状态，但只有 `diagnostic_log.rs` 自测之间共享了一把锁，其他测试没有参与这把锁，导致路径覆盖发生跨模块并行污染**

这也解释了为什么问题会在“workspace 默认并行”时暴露，而 `--test-threads=1` 稳定。

## Section 3：候选修复方案对比

### 方案 A：让测试可 override `MAX_LOG_BYTES`，把测试阈值降到例如 `8KB`

思路：

- 把 `MAX_LOG_BYTES` 从固定常量改成可注入或可测试覆盖
- 测试里不再写 `20MB + 1`，而是写一个很小但足以触发 rotate 的文件

优点：

- 能明显缩短 I/O 时间窗口
- 测试更快，CI 资源占用更小
- 即使最终仍需做串行化，这个改动本身也有长期收益

缺点：

- 需要动 production 代码接口或内部结构，给测试暴露一个 knob
- 它改善的是“窗口大小”，不是“共享全局状态”本身
- 如果真实根因是 `TEST_LOG_PATH` 被别的测试覆盖，阈值变小只能降低概率，不能从机制上消灭竞态

风险评估：

- 中等风险
- 风险不在功能行为，而在于把测试需求引入产线实现面，增加额外配置面

### 方案 B：在 `test_log_guard()` 持锁期间增加 `fs::sync_data` / 强制刷盘

思路：

- 在写完大文件或 rotate 后显式刷盘，让文件系统状态更快稳定

优点：

- 不一定需要改 production 逻辑
- 如果根因真是底层 I/O 可见性窗口，可能会降低复现概率

缺点：

- 不能解决“另一个测试改了 `TEST_LOG_PATH`”这个更像根因的问题
- 会拖慢测试
- 容易把症状压住，但没有真正隔离共享状态

风险评估：

- 中高风险
- 最大风险是掩盖根因，后面仍可能在别的机器或 CI 上继续偶发

### 方案 C：引入 `serial_test` crate，给相关测试加 `#[serial_test::serial]`

思路：

- 对所有触碰 `TEST_LOG_PATH` 的测试显式做进程内串行化

优点：

- 改动直接、行为明确
- 能把“共享全局测试状态”这个事实显式表达在测试定义上
- 对当前问题的针对性很强

缺点：

- 新增依赖
- 没有消除全局可变状态本身，只是给它外面套了统一的串行约束
- 如果未来有新测试也会改 `TEST_LOG_PATH`，但忘了加 serial 标记，问题会再次回来

风险评估：

- 低到中等风险
- 作为测试层修复较稳，但属于“约束使用者”的方案，不是“消灭共享点”的方案

### 方案 D：统一所有触碰 `TEST_LOG_PATH` 的测试隔离机制，避免跨模块裸写全局状态

思路：

- 保留现有产线行为不动
- 测试侧统一封装一个 guard 或 scoped helper，负责：
  - 设置 `TEST_LOG_PATH`
  - 在 guard 生命周期内持有同一把全局测试锁
  - 测试结束时恢复默认值
- `diagnostic_log.rs` 自己的测试和 `routes.rs` 中 `diagnostics_report_uses_ok_shape` 都改用这套统一入口

优点：

- 直接对准这次读源码发现的真正共享点
- 不需要为了测试速度改产线常量
- 不需要引入新依赖
- 能减少“某个测试忘记恢复全局值”这类隐患

缺点：

- 需要补齐测试辅助层，改到所有相关测试
- 仍然是串行化思路，只是比 `serial_test` 更内聚、更贴近当前代码

风险评估：

- 低风险
- 这是测试隔离层面的定向修复，和产线逻辑边界清晰

### 推荐方案

推荐优先考虑 **方案 D**。

理由：

- 它最贴近本次源码分析得到的高概率根因：`TEST_LOG_PATH` 是跨模块共享的全局可变状态，但当前串行约束只覆盖了 `diagnostic_log.rs` 自己的测试，没覆盖 `routes.rs` 里的相关测试。
- 相比方案 A，它不是“缩小时间窗口”，而是直接补齐共享状态的隔离边界。
- 相比方案 B，它不是用刷盘去碰运气。
- 相比方案 C，它不需要新依赖，也更能把“设置路径 + 恢复路径 + 串行化”收敛为一个统一约束，降低后续漏用概率。

如果维护者更偏好“5 分钟止血”，方案 C 也能接受；但从根因贴合度和长期可维护性看，D 更合适。
