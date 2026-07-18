//! Stable identifier newtypes, their construction rules, and the BLAKE3
//! semantic discriminator.
//!
//! Identifiers are deterministic strings derived from names / canonical paths,
//! never from array order, `DefaultHasher`, a process seed, absolute machine
//! paths, timestamps, or raw rustdoc numeric IDs. Where a semantic
//! discriminator is required (impl signatures, external-source disambiguation,
//! disambiguated relations), it is a BLAKE3 hash — see [`discriminator`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::kind::RelationKind;

macro_rules! string_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        pub struct $name(String);

        impl $name {
            /// Wraps an already-formed id string (e.g. when deserializing or in tests).
            pub fn from_raw(value: impl Into<String>) -> Self {
                $name(value.into())
            }

            /// The id as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(
    /// Stable identifier of an entity.
    EntityId
);
string_id!(
    /// Stable identifier of a relation.
    RelationId
);
string_id!(
    /// Stable identifier of a view.
    ViewId
);
string_id!(
    /// Stable identifier of a stage within a view.
    StageId
);

/// Length of the displayed hex discriminator (first 128 bits of a BLAKE3 hash).
const DISCRIMINATOR_HEX_LEN: usize = 32;

/// Domain/version prefix that separates CrateVista IDs from any other hash use.
const HASH_DOMAIN_PREFIX: &[u8] = b"cratevista-id:v1:";

/// Computes a deterministic semantic discriminator with BLAKE3.
///
/// The hash input is framed unambiguously so different component splits cannot
/// collide, and it is domain-separated:
///
/// ```text
/// "cratevista-id:v1:" <domain> ":" ( <byte_len> ":" <component_bytes> ":" )*
/// ```
///
/// where `<domain>` names the discriminator use (e.g. `impl-sig`, `pkg-src`,
/// `relation`) and each component is length-framed by its UTF-8 byte length.
/// The result is the first [`DISCRIMINATOR_HEX_LEN`] hex chars (128 bits). The
/// input must be built from normalized semantic values only — never from
/// iteration order, absolute paths, timestamps, or raw rustdoc IDs. Changing
/// this framing is a MAJOR schema change (see ADR-0003).
pub fn discriminator(domain: &str, components: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(HASH_DOMAIN_PREFIX);
    hasher.update(domain.as_bytes());
    hasher.update(b":");
    for component in components {
        hasher.update(component.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(component.as_bytes());
        hasher.update(b":");
    }
    let hex = hasher.finalize().to_hex();
    hex.as_str()[..DISCRIMINATOR_HEX_LEN].to_string()
}

impl EntityId {
    /// The single workspace entity id: `workspace`.
    pub fn workspace() -> Self {
        EntityId("workspace".to_string())
    }

    /// A workspace-member package: `package:{name}` (version excluded).
    pub fn package(name: &str) -> Self {
        EntityId(format!("package:{name}"))
    }

    /// An external package: `package:{name}@{version}`.
    pub fn external_package(name: &str, version: &str) -> Self {
        EntityId(format!("package:{name}@{version}"))
    }

    /// An external package that needs source disambiguation:
    /// `package:{name}@{version}:{discriminator}`, where the discriminator is a
    /// BLAKE3 over the normalized Cargo source identity (never an absolute path).
    pub fn external_package_disambiguated(
        name: &str,
        version: &str,
        source_kind: &str,
        normalized_source: &str,
    ) -> Self {
        let disc = discriminator("pkg-src", &[source_kind, normalized_source]);
        EntityId(format!("package:{name}@{version}:{disc}"))
    }

    /// A target: `target:{package}:{kind}:{name}`.
    pub fn target(package: &str, kind: &str, name: &str) -> Self {
        EntityId(format!("target:{package}:{kind}:{name}"))
    }

    /// A module: `module:{crate}::{module_path}`.
    pub fn module(krate: &str, module_path: &str) -> Self {
        EntityId(format!("module:{krate}::{module_path}"))
    }

    /// An item: `item:{kind}:{crate}::{canonical_path}`.
    pub fn item(kind: &str, krate: &str, canonical_path: &str) -> Self {
        EntityId(format!("item:{kind}:{krate}::{canonical_path}"))
    }

    /// An impl block: `impl:{crate}:{trait_or_inherent}:{for_type}:{discriminator}`.
    ///
    /// The discriminator is a BLAKE3 over the normalized impl signature and is
    /// always present, so multiple inherent impl blocks for one type cannot
    /// collide.
    pub fn impl_block(
        krate: &str,
        trait_or_inherent: &str,
        for_type: &str,
        normalized_signature: &str,
    ) -> Self {
        let disc = discriminator("impl-sig", &[normalized_signature]);
        EntityId(format!(
            "impl:{krate}:{trait_or_inherent}:{for_type}:{disc}"
        ))
    }

    /// A manual entity declared in configuration: `manual:{config_id}`.
    pub fn manual(config_id: &str) -> Self {
        EntityId(format!("manual:{config_id}"))
    }
}

impl ViewId {
    /// A view: `view:{name}`.
    pub fn view(name: &str) -> Self {
        ViewId(format!("view:{name}"))
    }
}

impl RelationId {
    /// Basic relation id: `rel:{kind}:{from}->{to}`.
    pub fn basic(kind: &RelationKind, from: &EntityId, to: &EntityId) -> Self {
        RelationId(format!(
            "rel:{}:{}->{}",
            kind.as_str(),
            from.as_str(),
            to.as_str()
        ))
    }

    /// Relation id with a semantic role: `rel:{kind}:{from}->{to}:{role}`.
    pub fn with_role(kind: &RelationKind, from: &EntityId, to: &EntityId, role: &str) -> Self {
        RelationId(format!(
            "rel:{}:{}->{}:{}",
            kind.as_str(),
            from.as_str(),
            to.as_str(),
            role
        ))
    }

    /// Relation id with a role and a BLAKE3 discriminator, used when the role
    /// alone is still insufficient: `rel:{kind}:{from}->{to}:{role}:{discriminator}`.
    pub fn with_role_and_discriminator(
        kind: &RelationKind,
        from: &EntityId,
        to: &EntityId,
        role: &str,
        discriminator_components: &[&str],
    ) -> Self {
        let disc = discriminator("relation", discriminator_components);
        RelationId(format!(
            "rel:{}:{}->{}:{}:{}",
            kind.as_str(),
            from.as_str(),
            to.as_str(),
            role,
            disc
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_id_formats() {
        assert_eq!(EntityId::workspace().as_str(), "workspace");
        assert_eq!(EntityId::package("foo").as_str(), "package:foo");
        assert_eq!(
            EntityId::external_package("serde", "1.0.0").as_str(),
            "package:serde@1.0.0"
        );
        assert_eq!(
            EntityId::target("foo", "lib", "foo").as_str(),
            "target:foo:lib:foo"
        );
        assert_eq!(
            EntityId::module("foo", "bar::baz").as_str(),
            "module:foo::bar::baz"
        );
        assert_eq!(
            EntityId::item("struct", "foo", "bar::Baz").as_str(),
            "item:struct:foo::bar::Baz"
        );
        assert_eq!(EntityId::manual("web-client").as_str(), "manual:web-client");
    }

    #[test]
    fn relation_id_basic_and_roles() {
        let from = EntityId::package("a");
        let to = EntityId::package("b");
        let kind = RelationKind::new(RelationKind::DEPENDS_ON);
        assert_eq!(
            RelationId::basic(&kind, &from, &to).as_str(),
            "rel:depends_on:package:a->package:b"
        );
        assert_eq!(
            RelationId::with_role(&kind, &from, &to, "build").as_str(),
            "rel:depends_on:package:a->package:b:build"
        );
    }

    #[test]
    fn discriminator_is_deterministic_and_128_bit() {
        let a = discriminator("impl-sig", &["impl Foo for Bar"]);
        let b = discriminator("impl-sig", &["impl Foo for Bar"]);
        assert_eq!(a, b);
        assert_eq!(a.len(), DISCRIMINATOR_HEX_LEN);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        // Length framing prevents component-split collisions.
        assert_ne!(
            discriminator("d", &["ab", "c"]),
            discriminator("d", &["a", "bc"])
        );
        // Domain separation matters.
        assert_ne!(
            discriminator("impl-sig", &["x"]),
            discriminator("pkg-src", &["x"])
        );
    }

    #[test]
    fn impl_id_includes_signature_discriminator() {
        let a = EntityId::impl_block("foo", "inherent", "Bar", "fn a()");
        let b = EntityId::impl_block("foo", "inherent", "Bar", "fn b()");
        assert_ne!(a, b, "distinct inherent impl blocks must not collide");
        assert!(a.as_str().starts_with("impl:foo:inherent:Bar:"));
    }
}
