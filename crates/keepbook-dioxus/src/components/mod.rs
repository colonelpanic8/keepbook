//! Presentational components shared by the views.
//!
//! Each component lives in `<group>/<name>.rs` and has a React mirror at
//! `design/src/<group>/<Name>.tsx`, which is synced to Claude Design. The
//! parity tests render both and fail when their markup differs.

mod actions;
mod charts;
mod display;
mod feedback;
mod forms;
mod layout;

pub(crate) use actions::*;
pub(crate) use charts::*;
pub(crate) use display::*;
pub(crate) use feedback::*;
pub(crate) use forms::*;
pub(crate) use layout::*;
