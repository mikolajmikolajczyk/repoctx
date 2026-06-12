import React from "react";

export type Props = { label: string };

export const Button = (p: Props) => <button>{p.label}</button>;

export function Panel(): JSX.Element {
  return <div />;
}
