//! The reference Profile.vue is a title-only page; UserCapsule is not mounted.
use super::*;
impl Mio {
    pub(super) fn profile_page(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(20.)).child(
            div()
                .text_size(px(24.))
                .font_weight(FontWeight::BOLD)
                .child(self.t("auth.profile")),
        )
    }
}
