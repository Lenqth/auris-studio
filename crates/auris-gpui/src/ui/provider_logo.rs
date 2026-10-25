//! Embedded provider marks shared by settings and the model picker.

use std::borrow::Cow;

use gpui::{AssetSource, Hsla, IntoElement, SharedString, div, prelude::*, svg};

const MARKS: &[(&str, &[u8])] = &[
    (
        "providers/openai.svg",
        include_bytes!("../../assets/providers/openai.svg"),
    ),
    (
        "providers/claude.svg",
        include_bytes!("../../assets/providers/claude.svg"),
    ),
    (
        "providers/opencode.svg",
        include_bytes!("../../assets/providers/opencode.svg"),
    ),
    (
        "providers/antigravity.svg",
        include_bytes!("../../assets/providers/antigravity.svg"),
    ),
    (
        "providers/ollama.svg",
        include_bytes!("../../assets/providers/ollama.svg"),
    ),
];

/// Provider artwork included in the executable, with no runtime downloads.
pub(crate) struct ProviderAssets;

impl AssetSource for ProviderAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(MARKS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(MARKS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| (*name).into())
            .collect())
    }
}

/// Resolve the connection provider rather than guessing from a model's name.
pub(crate) fn provider_id<'a>(backend: &'a str, provider: &'a str) -> &'a str {
    match backend {
        "codex" => "openai",
        "claude" | "opencode" | "antigravity" => backend,
        _ if provider == "ollama" => "ollama",
        _ => "api",
    }
}

/// A monochrome provider mark that follows the current theme's foreground.
pub(crate) fn provider_logo(provider: &str, color: Hsla) -> impl IntoElement + use<> {
    let path = match provider {
        "openai" => Some("providers/openai.svg"),
        "claude" => Some("providers/claude.svg"),
        "opencode" => Some("providers/opencode.svg"),
        "antigravity" => Some("providers/antigravity.svg"),
        "ollama" => Some("providers/ollama.svg"),
        _ => None,
    };
    div()
        .size_5()
        .flex_shrink_0()
        .text_color(color)
        .when_some(path, |view, path| {
            view.child(svg().path(path).size_full().text_color(color))
        })
        .when(path.is_none(), |view| {
            view.flex()
                .items_center()
                .justify_center()
                .text_xs()
                .child("API")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn embedded_marks_render_without_external_files(cx: &mut gpui::TestAppContext) {
        let renderer = cx.update(|cx| cx.svg_renderer());
        for (path, _) in MARKS {
            let bytes = ProviderAssets.load(path).unwrap().unwrap();
            let image = gpui::Image::from_bytes(gpui::ImageFormat::Svg, bytes.into_owned())
                .to_image_data(renderer.clone())
                .unwrap();
            assert!(
                image
                    .as_bytes(0)
                    .unwrap()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] > 0),
                "{path} is empty"
            );
        }
    }

    #[test]
    fn an_api_dialect_does_not_claim_to_be_the_openai_provider() {
        assert_eq!(provider_id("rig", "openai"), "api");
        assert_eq!(provider_id("codex", "ollama"), "openai");
        assert_eq!(provider_id("claude", "openai"), "claude");
    }
}
