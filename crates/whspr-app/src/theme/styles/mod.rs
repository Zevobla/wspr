//! iced widget `style` functions built on the MD3 tokens in
//! `crate::theme`. Each submodule covers one widget kind; `crate::hub`
//! calls these instead of iced's built-in theme styles, so every surface
//! renders from the same MD3 color/shape tokens.

pub mod button;
pub mod container;
pub mod pick_list;
pub mod scrollable;
pub mod slider;
pub mod text_input;
