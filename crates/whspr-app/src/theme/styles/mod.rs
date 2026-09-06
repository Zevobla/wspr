//! iced widget `style` functions built on the MD3 tokens in
//! `crate::theme`. Each submodule covers one widget kind; `crate::hub`/
//! `crate::flow_bar` call these instead of iced's built-in theme styles,
//! so every surface renders from the same MD3 color/shape tokens.

pub mod button;
pub mod checkbox;
pub mod container;
pub mod pick_list;
pub mod progress_bar;
pub mod scrollable;
pub mod text_input;

#[cfg(test)]
mod tests {
    #[test]
    fn style_modules_are_accessible() {
        // Verify that all style modules compile and are accessible.
        // This ensures the re-exports in this module are correct.
        let _button = super::button::filled;
        let _checkbox = super::checkbox::default;
        let _container = super::container::surface;
        let _pick_list = super::pick_list::active;
        let _progress_bar = super::progress_bar::default;
        let _scrollable = super::scrollable::active;
        let _text_input = super::text_input::active;
    }
}
