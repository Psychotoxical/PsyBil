import { Fragment, type ReactNode } from "react";

/**
 * A translated sentence with `{name}` marks replaced by elements, so a date
 * or an amount inside it can be set in the number font. Pass the template
 * from `t(key, { name: "{name}" })`, which keeps the marks.
 */
export function rich(template: string, nodes: Record<string, ReactNode>): ReactNode {
  return template.split(/(\{\w+\})/).map((part, i) => {
    const name = /^\{(\w+)\}$/.exec(part)?.[1];
    return (
      // biome-ignore lint/suspicious/noArrayIndexKey: the parts of one sentence never move
      <Fragment key={i}>{name && name in nodes ? nodes[name] : part}</Fragment>
    );
  });
}

/** The marks themselves as values, so `t()` hands back the template. */
export const marks = (...names: string[]) =>
  Object.fromEntries(names.map((n) => [n, `{${n}}`])) as Record<string, string>;
