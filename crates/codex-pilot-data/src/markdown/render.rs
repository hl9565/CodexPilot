use super::{
    Message, MessageBlock, TextPart, exported_at_label, parse_time_hhmm, split_fenced_code,
    strip_image_tags,
};

pub(super) fn render_markdown(title: &str, messages: &[Message]) -> String {
    let mut lines = vec![format!("# {title}"), String::new()];
    if messages.is_empty() {
        lines.push("_No messages found._".to_string());
        lines.push(String::new());
    }
    for message in messages {
        lines.push(format!("## {}", message.speaker));
        if let Some(timestamp) = message.timestamp.as_ref().filter(|value| !value.is_empty()) {
            lines.push(format!("_{timestamp}_"));
        }
        lines.push(String::new());
        lines.push(render_markdown_body(&message.blocks));
        lines.push(String::new());
    }
    format!("{}\n", lines.join("\n").trim_end())
}

pub(super) fn render_html(title: &str, messages: &[Message]) -> String {
    let exported_at = exported_at_label();
    let mut sections = String::new();
    if messages.is_empty() {
        sections.push_str(r#"<section class="empty">No messages found.</section>"#);
    }
    for message in messages {
        let timestamp = message
            .timestamp
            .as_ref()
            .filter(|value| !value.is_empty())
            .map(|value| {
                format!(
                    r#"<span class="time">{}</span>"#,
                    escape_html(&display_message_time(value))
                )
            })
            .unwrap_or_default();
        sections.push_str(&format!(
            r#"<section class="message {role_class}"><div class="avatar" aria-hidden="true">{avatar}</div><div class="bubble-wrap"><div class="speaker">{speaker}{timestamp}</div><div class="bubble">{body}</div></div></section>"#,
            role_class = role_class(&message.speaker),
            avatar = avatar_markup(&message.speaker),
            speaker = escape_html(&display_speaker_label(&message.speaker)),
            timestamp = timestamp,
            body = render_html_body(&message.blocks)
        ));
    }
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <style>
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      background: #f6f8fb;
      color: #1f2937;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      line-height: 1.65;
    }}
    .page {{
      background: #fff;
      border: 1px solid #dde5ee;
      border-radius: 12px;
      box-shadow: 0 18px 48px rgba(15, 23, 42, 0.10);
      margin: 32px auto;
      max-width: 920px;
      overflow: hidden;
    }}
    header {{
      border-bottom: 1px solid #e5ebf2;
      padding: 28px 34px 22px;
    }}
    .brand {{
      color: #526174;
      font-size: 12px;
      font-weight: 800;
      margin-bottom: 10px;
    }}
    h1 {{
      font-size: 28px;
      line-height: 1.25;
      margin: 0 0 10px;
    }}
    .meta {{
      color: #66758a;
      display: flex;
      flex-wrap: wrap;
      font-size: 13px;
      gap: 12px;
    }}
    main {{
      background: #f8fafc;
      padding: 18px 34px 34px;
    }}
    .message {{
      align-items: flex-start;
      display: flex;
      gap: 10px;
      margin: 18px 0;
    }}
    .message.user {{
      flex-direction: row-reverse;
    }}
    .bubble-wrap {{
      max-width: min(680px, calc(100% - 54px));
      min-width: 0;
    }}
    .speaker {{
      color: #64748b;
      font-size: 12px;
      font-weight: 700;
      margin: 0 0 6px;
    }}
    .user .speaker {{
      text-align: right;
    }}
    .time {{
      color: #94a3b8;
      display: inline;
      font-size: 12px;
      font-weight: 600;
      margin-left: 8px;
    }}
    .avatar {{
      align-items: center;
      background: #e2e8f0;
      border: 1px solid #cbd5e1;
      border-radius: 50%;
      color: #475569;
      display: flex;
      flex: 0 0 36px;
      height: 36px;
      justify-content: center;
      margin-top: 24px;
      width: 36px;
    }}
    .user .avatar {{
      background: #e0f2fe;
      border-color: #bae6fd;
      color: #0369a1;
    }}
    .assistant .avatar {{
      background: #eef2ff;
      border-color: #c7d2fe;
      color: #4338ca;
    }}
    .avatar svg {{
      height: 19px;
      width: 19px;
    }}
    .bubble {{
      background: #fff;
      border: 1px solid #e2e8f0;
      border-radius: 8px;
      box-shadow: 0 8px 20px rgba(15, 23, 42, 0.05);
      color: #1f2937;
      font-size: 14px;
      min-width: 0;
      overflow: hidden;
      padding: 14px 16px;
    }}
    .user .bubble {{
      background: #eef8ff;
      border-color: #cfe8f8;
    }}
    .text {{
      overflow-wrap: anywhere;
      white-space: pre-wrap;
    }}
    .text + .text,
    .text + .image-block,
    .text + .code-block,
    .image-block + .text,
    .image-block + .image-block,
    .image-block + .code-block,
    .code-block + .text,
    .code-block + .image-block,
    .code-block + .code-block {{
      margin-top: 12px;
    }}
    .code-block {{
      background: #0f172a;
      border-radius: 8px;
      color: #e5e7eb;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
      line-height: 1.55;
      margin: 0;
      overflow-x: auto;
      padding: 12px 14px;
      white-space: pre;
    }}
    .image-block {{
      align-items: center;
      background: #f8fafc;
      border: 1px solid #e2e8f0;
      border-radius: 8px;
      color: #64748b;
      display: inline-flex;
      font-size: 13px;
      font-weight: 700;
      gap: 8px;
      margin: 0;
      max-width: 100%;
      min-height: 44px;
      padding: 10px 12px;
    }}
    .image-block img {{
      border-radius: 6px;
      display: block;
      max-height: 360px;
      max-width: 100%;
    }}
    .empty {{
      color: #66758a;
      padding: 24px 0 0;
    }}
    @media (max-width: 720px) {{
      .page {{ border-left: 0; border-right: 0; border-radius: 0; margin: 0; }}
      header, main {{ padding-left: 20px; padding-right: 20px; }}
      .bubble-wrap {{ max-width: calc(100% - 48px); }}
    }}
  </style>
</head>
<body>
  <article class="page">
    <header>
      <div class="brand">CodexPilot Export</div>
      <h1>{title}</h1>
      <div class="meta">
        <span>Exported {exported_at}</span>
        <span>{message_count} messages</span>
      </div>
    </header>
    <main>
      {sections}
    </main>
  </article>
</body>
</html>
"#,
        title = escape_html(title),
        exported_at = escape_html(&exported_at),
        message_count = messages.len(),
        sections = sections
    )
}

fn render_markdown_body(blocks: &[MessageBlock]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            MessageBlock::Text(text) => text.trim().to_string(),
            MessageBlock::Image(_) => "> Image attachment".to_string(),
        })
        .filter(|block| !block.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_html_body(blocks: &[MessageBlock]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            MessageBlock::Text(text) => render_html_text(text),
            MessageBlock::Image(src) => render_image_block(src.as_deref()),
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_html_text(value: &str) -> String {
    split_fenced_code(&strip_image_tags(value))
        .into_iter()
        .map(|block| match block {
            TextPart::Plain(text) => {
                format!(r#"<div class="text">{}</div>"#, escape_html(text.trim()))
            }
            TextPart::Code(code) => format!(
                r#"<pre class="code-block"><code>{}</code></pre>"#,
                escape_html(code.trim())
            ),
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_image_block(src: Option<&str>) -> String {
    match src {
        Some(src) => format!(
            r#"<figure class="image-block"><img src="{}" alt="Image attachment"></figure>"#,
            escape_html(src)
        ),
        None => format!(
            r#"<div class="image-block">{icon}<span>图片附件</span></div>"#,
            icon = image_icon()
        ),
    }
}

fn role_class(speaker: &str) -> &'static str {
    match speaker {
        "User" => "user",
        "Assistant" => "assistant",
        _ => "system",
    }
}

fn display_speaker_label(speaker: &str) -> &str {
    match speaker {
        "User" => "You",
        "Assistant" => "AI",
        value => value,
    }
}

fn avatar_markup(speaker: &str) -> &'static str {
    match speaker {
        "Assistant" => robot_icon(),
        "User" => user_icon(),
        _ => system_icon(),
    }
}

fn robot_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 8V4"/><rect x="5" y="8" width="14" height="10" rx="3"/><path d="M8 14h.01"/><path d="M16 14h.01"/><path d="M9 18v2h6v-2"/></svg>"#
}

fn user_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="4"/><path d="M5 21a7 7 0 0 1 14 0"/></svg>"#
}

fn system_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M12 2v3"/><path d="M12 19v3"/><path d="M2 12h3"/><path d="M19 12h3"/></svg>"#
}

fn image_icon() -> &'static str {
    r#"<svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="14" rx="2"/><circle cx="8.5" cy="10" r="1.5"/><path d="m21 15-5-5L5 19"/></svg>"#
}

fn display_message_time(value: &str) -> String {
    parse_time_hhmm(value).unwrap_or_else(|| value.to_string())
}

fn escape_html(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(ch),
        }
    }
    output
}
