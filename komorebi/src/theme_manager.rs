#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::border_manager;
use crate::stackbar_manager;
use crate::Colour;
use crate::KomorebiTheme;
use crate::WindowManager;

impl WindowManager {
    /// Updates the colors from the `BorderManager` and the `StackbarManager` according to
    /// the new `theme`
    pub fn update_theme(&mut self, theme: KomorebiTheme) {
        if self.theme == Some(theme) {
            // Theme is already applied, so we can ignore it
            tracing::trace!("ignoring already applied theme {:?}", &theme);
            return;
        }

        let (
            single_border,
            stack_border,
            monocle_border,
            floating_border,
            unfocused_border,
            stackbar_focused_text,
            stackbar_unfocused_text,
            stackbar_background,
        ) = match theme {
            KomorebiTheme::Catppuccin {
                name,
                single_border,
                stack_border,
                monocle_border,
                floating_border,
                unfocused_border,
                stackbar_focused_text,
                stackbar_unfocused_text,
                stackbar_background,
                ..
            } => {
                let single_border = single_border
                    .unwrap_or(komorebi_themes::CatppuccinValue::Blue)
                    .color32(name.as_theme());

                let stack_border = stack_border
                    .unwrap_or(komorebi_themes::CatppuccinValue::Green)
                    .color32(name.as_theme());

                let monocle_border = monocle_border
                    .unwrap_or(komorebi_themes::CatppuccinValue::Pink)
                    .color32(name.as_theme());

                let floating_border = floating_border
                    .unwrap_or(komorebi_themes::CatppuccinValue::Yellow)
                    .color32(name.as_theme());

                let unfocused_border = unfocused_border
                    .unwrap_or(komorebi_themes::CatppuccinValue::Base)
                    .color32(name.as_theme());

                let stackbar_focused_text = stackbar_focused_text
                    .unwrap_or(komorebi_themes::CatppuccinValue::Green)
                    .color32(name.as_theme());

                let stackbar_unfocused_text = stackbar_unfocused_text
                    .unwrap_or(komorebi_themes::CatppuccinValue::Text)
                    .color32(name.as_theme());

                let stackbar_background = stackbar_background
                    .unwrap_or(komorebi_themes::CatppuccinValue::Base)
                    .color32(name.as_theme());

                (
                    single_border,
                    stack_border,
                    monocle_border,
                    floating_border,
                    unfocused_border,
                    stackbar_focused_text,
                    stackbar_unfocused_text,
                    stackbar_background,
                )
            }
            KomorebiTheme::Base16 {
                name,
                single_border,
                stack_border,
                monocle_border,
                floating_border,
                unfocused_border,
                stackbar_focused_text,
                stackbar_unfocused_text,
                stackbar_background,
                ..
            } => {
                let single_border = single_border
                    .unwrap_or(komorebi_themes::Base16Value::Base0D)
                    .color32(name);

                let stack_border = stack_border
                    .unwrap_or(komorebi_themes::Base16Value::Base0B)
                    .color32(name);

                let monocle_border = monocle_border
                    .unwrap_or(komorebi_themes::Base16Value::Base0F)
                    .color32(name);

                let unfocused_border = unfocused_border
                    .unwrap_or(komorebi_themes::Base16Value::Base01)
                    .color32(name);

                let floating_border = floating_border
                    .unwrap_or(komorebi_themes::Base16Value::Base09)
                    .color32(name);

                let stackbar_focused_text = stackbar_focused_text
                    .unwrap_or(komorebi_themes::Base16Value::Base0B)
                    .color32(name);

                let stackbar_unfocused_text = stackbar_unfocused_text
                    .unwrap_or(komorebi_themes::Base16Value::Base05)
                    .color32(name);

                let stackbar_background = stackbar_background
                    .unwrap_or(komorebi_themes::Base16Value::Base01)
                    .color32(name);

                (
                    single_border,
                    stack_border,
                    monocle_border,
                    floating_border,
                    unfocused_border,
                    stackbar_focused_text,
                    stackbar_unfocused_text,
                    stackbar_background,
                )
            }
        };

        self.border_manager.kind_colours.single_colour = u32::from(Colour::from(single_border));
        self.border_manager.kind_colours.monocle_colour = u32::from(Colour::from(monocle_border));
        self.border_manager.kind_colours.stack_colour = u32::from(Colour::from(stack_border));
        self.border_manager.kind_colours.floating_colour = u32::from(Colour::from(floating_border));
        self.border_manager.kind_colours.unfocused_colour = u32::from(Colour::from(unfocused_border));

        self.stackbar_manager.globals.tab_background_colour = u32::from(Colour::from(stackbar_background));
        self.stackbar_manager.globals.focused_text_colour = u32::from(Colour::from(stackbar_focused_text));
        self.stackbar_manager.globals.unfocused_text_colour = u32::from(Colour::from(stackbar_unfocused_text));

        self.theme = Some(theme);

        border_manager::send_notification(None);
        stackbar_manager::send_update();
    }
}
