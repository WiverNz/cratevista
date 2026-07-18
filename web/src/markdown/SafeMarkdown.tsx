// Sanitized Markdown renderer for rustdoc descriptions (which may contain raw
// HTML from third-party crate docs). react-markdown + remark-gfm +
// rehype-sanitize; NO rehype-raw, NO dangerouslySetInnerHTML. External links are
// hardened with rel="noopener noreferrer".
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeSanitize from "rehype-sanitize";
import type { AnchorHTMLAttributes } from "react";

function isSafeHref(href: string | undefined): boolean {
  if (!href) return false;
  const trimmed = href.trim().toLowerCase();
  // Block javascript:/data:/vbscript: (incl. common encodings handled by
  // decodeURIComponent); allow http(s), mailto, fragment, and relative.
  let decoded = trimmed;
  try {
    decoded = decodeURIComponent(trimmed);
  } catch {
    // keep raw trimmed on malformed encoding
  }
  return !/^\s*(javascript|data|vbscript):/i.test(decoded);
}

function SafeLink(props: AnchorHTMLAttributes<HTMLAnchorElement>) {
  const { href, children, ...rest } = props;
  if (!isSafeHref(href)) {
    return <span {...rest}>{children}</span>;
  }
  const external = /^https?:\/\//i.test(href!.trim());
  return (
    <a
      {...rest}
      href={href}
      {...(external ? { target: "_blank", rel: "noopener noreferrer" } : {})}
    >
      {children}
    </a>
  );
}

export interface SafeMarkdownProps {
  children: string;
}

export function SafeMarkdown({ children }: SafeMarkdownProps) {
  return (
    <Markdown
      remarkPlugins={[remarkGfm]}
      rehypePlugins={[rehypeSanitize]}
      components={{ a: SafeLink }}
    >
      {children}
    </Markdown>
  );
}
