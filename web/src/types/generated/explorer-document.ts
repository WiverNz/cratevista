/**
 * GENERATED FILE — DO NOT EDIT.
 *
 * Produced by `npm run generate:types` from
 * crates/cratevista-schema/schema/cratevista-document.schema.json (PRD 02).
 * Regenerate after any schema change; `npm run check:types` fails when stale.
 */
/* eslint-disable */

/**
 * Stable identifier of an entity.
 */
export type EntityId = string;
/**
 * The classification of an [`crate::entity::Entity`]. Open and string-backed.
 */
export type EntityKind = string;
/**
 * The classification of a [`crate::relation::Relation`]. Open and string-backed.
 */
export type RelationKind = string;

/**
 * The canonical explorer document serialized to `document.json`.
 *
 * Deterministic: it carries no timestamps, no runtime metadata, and no
 * diagnostics (those are separate `generation.json` / `diagnostics.json`
 * artifacts). Entities, relations, and views are sorted by id.
 */
export interface ExplorerDocument {
  /**
   * Entities, sorted by id.
   */
  entities: Entity[];
  project: Project;
  /**
   * Relations, sorted by id.
   */
  relations: Relation[];
  /**
   * Schema version of this artifact.
   */
  schema_version: string;
  /**
   * Views, sorted by id.
   */
  views: View[];
  [k: string]: unknown;
}
/**
 * A node in the explorer graph.
 */
export interface Entity {
  /**
   * Freeform attributes.
   */
  attributes?: {
    [k: string]: unknown;
  };
  /**
   * Optional longer description (localization-ready).
   */
  description?: LocalizedText | null;
  /**
   * Optional documentation block.
   */
  docs?: DocBlock | null;
  /**
   * Stable identifier.
   */
  id: string;
  /**
   * Open, string-backed kind.
   */
  kind: string;
  label: LocalizedText1;
  /**
   * Optional containing entity.
   */
  parent?: EntityId | null;
  /**
   * Discovered vs manual provenance.
   */
  provenance: "discovered" | "manual";
  /**
   * Fully-qualified name (e.g. `crate::module::Type`).
   */
  qualified_name: string;
  /**
   * Optional repository-relative source location.
   */
  source?: SourceLocation | null;
  /**
   * Ordered, deduplicated tags.
   */
  tags?: string[];
  [k: string]: unknown;
}
/**
 * Localization-ready text: a default string plus optional per-language
 * translations keyed by language code.
 */
export interface LocalizedText {
  /**
   * The default (source-language) text.
   */
  default: string;
  /**
   * Optional translations keyed by language code (e.g. `ru`).
   */
  translations?: {
    [k: string]: string;
  };
  [k: string]: unknown;
}
/**
 * A rustdoc/Markdown documentation block for an entity.
 */
export interface DocBlock {
  /**
   * Whether the underlying item is documented (used for coverage).
   */
  documented: boolean;
  /**
   * The full documentation as Markdown.
   */
  markdown: string;
  /**
   * An optional short summary (e.g. the first paragraph).
   */
  summary?: string | null;
  [k: string]: unknown;
}
/**
 * Localization-ready text: a default string plus optional per-language
 * translations keyed by language code.
 */
export interface LocalizedText1 {
  /**
   * The default (source-language) text.
   */
  default: string;
  /**
   * Optional translations keyed by language code (e.g. `ru`).
   */
  translations?: {
    [k: string]: string;
  };
  [k: string]: unknown;
}
/**
 * A repository-relative source location with an optional span.
 */
export interface SourceLocation {
  /**
   * The validated repository-relative path.
   */
  path: string;
  /**
   * Optional 1-based line/column span.
   */
  span?: Span | null;
  [k: string]: unknown;
}
/**
 * A 1-based line/column span within a source file.
 */
export interface Span {
  /**
   * 1-based end column.
   */
  end_col: number;
  /**
   * 1-based end line.
   */
  end_line: number;
  /**
   * 1-based start column.
   */
  start_col: number;
  /**
   * 1-based start line.
   */
  start_line: number;
  [k: string]: unknown;
}
/**
 * Project/workspace metadata.
 */
export interface Project {
  /**
   * Optional default branch.
   */
  default_branch?: string | null;
  /**
   * Short description.
   */
  description: string;
  /**
   * Stable project id.
   */
  id: string;
  /**
   * Human-readable project name.
   */
  name: string;
  /**
   * Optional repository URL.
   */
  repository_url?: string | null;
  /**
   * Optional repository-relative root location.
   */
  root?: SourceLocation | null;
  [k: string]: unknown;
}
/**
 * A typed, directed edge between two entities.
 */
export interface Relation {
  /**
   * Freeform attributes (e.g. protocol labels such as HTTP/SQL).
   */
  attributes?: {
    [k: string]: unknown;
  };
  /**
   * Stable identifier of an entity.
   */
  from: string;
  /**
   * Stable identifier.
   */
  id: string;
  /**
   * Open, string-backed kind.
   */
  kind: string;
  /**
   * Optional edge label (localization-ready).
   */
  label?: LocalizedText | null;
  /**
   * Discovered vs manual provenance.
   */
  provenance: "discovered" | "manual";
  /**
   * Optional semantic role, disambiguating multiple same-kind relations
   * between the same endpoints.
   */
  role?: string | null;
  /**
   * Stable identifier of an entity.
   */
  to: string;
  [k: string]: unknown;
}
/**
 * A named projection over the canonical entities/relations.
 *
 * Views carry filters and presentation metadata but never UI coordinates
 * (layout is computed client-side).
 */
export interface View {
  /**
   * Optional default focus entity.
   */
  default_focus?: EntityId | null;
  /**
   * Optional description (localization-ready).
   */
  description?: LocalizedText | null;
  /**
   * View-level documentation (Markdown), e.g. what a manual flow describes.
   *
   * Optional and absent on the generated views; added in schema `1.1`.
   */
  docs?: DocBlock | null;
  /**
   * Explicit membership (else membership is derived from the filters).
   */
  entity_ids?: EntityId[] | null;
  /**
   * Entity-kind filter.
   */
  entity_kinds?: EntityKind[];
  /**
   * Worked examples whose contents are embedded in the document.
   *
   * Optional and empty on the generated views; added in schema `1.1`.
   */
  examples?: ViewExample[];
  /**
   * Stable identifier.
   */
  id: string;
  /**
   * Presentation hints (legend/grouping); never coordinates.
   */
  presentation?: {
    [k: string]: unknown;
  };
  /**
   * Relation-kind filter.
   */
  relation_kinds?: RelationKind[];
  /**
   * Ordered stages/groups.
   */
  stages?: Stage[];
  title: LocalizedText4;
  [k: string]: unknown;
}
/**
 * A worked example attached to a view (e.g. sample output for a manual flow).
 *
 * The example's [`content`](ViewExample::content) is **embedded** in the
 * document at generation time rather than referenced by path, so the explorer
 * renders it without the guarded `/api/source` endpoint and a static export is
 * self-contained. Producers embed only files a maintainer named explicitly in
 * configuration.
 */
export interface ViewExample {
  /**
   * The example content, embedded verbatim (UTF-8).
   */
  content: string;
  /**
   * Optional prose about the example (localization-ready).
   */
  description?: LocalizedText | null;
  /**
   * Stable identifier, unique within the view.
   */
  id: string;
  /**
   * Syntax hint for display only (e.g. `json`, `http`, `sql`). Never
   * interpreted or executed.
   */
  language?: string | null;
  title: LocalizedText2;
  [k: string]: unknown;
}
/**
 * Localization-ready text: a default string plus optional per-language
 * translations keyed by language code.
 */
export interface LocalizedText2 {
  /**
   * The default (source-language) text.
   */
  default: string;
  /**
   * Optional translations keyed by language code (e.g. `ru`).
   */
  translations?: {
    [k: string]: string;
  };
  [k: string]: unknown;
}
/**
 * An ordered stage/group within a view (e.g. a flow step lane).
 */
export interface Stage {
  /**
   * Stable identifier.
   */
  id: string;
  /**
   * Ordering index within the view.
   */
  order: number;
  title: LocalizedText3;
  [k: string]: unknown;
}
/**
 * Localization-ready text: a default string plus optional per-language
 * translations keyed by language code.
 */
export interface LocalizedText3 {
  /**
   * The default (source-language) text.
   */
  default: string;
  /**
   * Optional translations keyed by language code (e.g. `ru`).
   */
  translations?: {
    [k: string]: string;
  };
  [k: string]: unknown;
}
/**
 * Localization-ready text: a default string plus optional per-language
 * translations keyed by language code.
 */
export interface LocalizedText4 {
  /**
   * The default (source-language) text.
   */
  default: string;
  /**
   * Optional translations keyed by language code (e.g. `ru`).
   */
  translations?: {
    [k: string]: string;
  };
  [k: string]: unknown;
}
