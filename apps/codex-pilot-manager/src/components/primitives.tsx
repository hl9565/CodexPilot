import * as React from "react";
import { type ProviderCount } from "../types";

export function Distribution({ title, items }: { title: string; items: ProviderCount[] }) {
  return (
    <div className="distributionBox">
      <strong>{title}</strong>
      <div>
        {items.length ? items.map((item) => (
          <span className="providerChip" key={item.provider}>
            {item.provider || "空"} {item.count}
          </span>
        )) : <span className="bodyText">无</span>}
      </div>
    </div>
  );
}

export function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

export function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="row">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
