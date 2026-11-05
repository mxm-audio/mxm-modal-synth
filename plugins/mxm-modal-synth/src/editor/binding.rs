//! Binding one parameter to one `mxm-ui` control: **the collection's one binding**,
//! `mxm_params::binding` (`plans/plan-editor-standard.md` R1c). This module re-exports it, as every
//! editor's does, so `super::binding::…` paths read the same in each.

pub use mxm_params::binding::*;
