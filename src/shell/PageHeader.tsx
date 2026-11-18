import type { ReactNode } from "react";

type Props = { title: string; sub?: string; actions?: ReactNode };

/** The title of a page, what it is about, and its main actions. */
export function PageHeader({ title, sub, actions }: Props) {
  return (
    <header className="page-header">
      <div>
        <h1>{title}</h1>
        {sub && <p className="page-sub">{sub}</p>}
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </header>
  );
}
