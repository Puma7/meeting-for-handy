import { Fragment } from "react";

/** Render the original section verbatim, marking only exact occurrences of its citation. */
export function LiteralQuote({
  text,
  quote,
}: {
  text: string;
  quote?: string;
}) {
  if (!quote || !text.includes(quote)) return <>{text}</>;
  return (
    <>
      {text.split(quote).map((part, index, pieces) => (
        <Fragment key={index}>
          {part}
          {index < pieces.length - 1 && <mark>{quote}</mark>}
        </Fragment>
      ))}
    </>
  );
}
