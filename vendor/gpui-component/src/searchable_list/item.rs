use gpui::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels,
    RenderOnce, StyleRefinement, Styled, Window, prelude::FluentBuilder,
};

use crate::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, Size, StyleSized, StyledExt,
    h_flex,
};

/// Optional dropdown presentation shared by the surface and its option rows.
#[derive(Clone)]
pub struct DropdownStyle {
    pub surface: StyleRefinement,
    pub item: StyleRefinement,
    pub selected: StyleRefinement,
    pub hover: StyleRefinement,
    pub padding: Pixels,
    pub spacing: Pixels,
    pub offset: Pixels,
    pub show_check: bool,
}

/// A single row element used inside searchable-list dropdowns (Select, ComboBox, MultiComboBox).
///
/// - `selected` — controls the cursor-highlight background (the `List` overwrites this field via
///   `Selectable::selected` to match the keyboard cursor position).
/// - `checked` — controls the visibility of the trailing check icon; set by the adapter based on
///   the current selection state and NOT overwritten by the `List`.
#[derive(IntoElement)]
pub struct SearchableListItemElement {
    id: ElementId,
    size: Size,
    style: StyleRefinement,
    /// Cursor/highlight background (overridden by `List` to the keyboard cursor row).
    selected: bool,
    /// Whether the trailing check icon is shown.
    checked: bool,
    disabled: bool,
    children: Vec<AnyElement>,
    /// The icon drawn at the trailing edge when `checked` is `true`.
    check_icon: Option<Icon>,
    pub(crate) dropdown_style: Option<DropdownStyle>,
}

impl SearchableListItemElement {
    pub fn new(ix: usize) -> Self {
        Self {
            id: ("searchable-list-item", ix).into(),
            size: Size::default(),
            style: StyleRefinement::default(),
            selected: false,
            checked: false,
            disabled: false,
            children: Vec::new(),
            check_icon: Some(Icon::new(IconName::Check)),
            dropdown_style: None,
        }
    }

    /// Set whether the trailing check icon is visible.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Override the default check icon.
    pub fn check_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.check_icon = Some(icon.into());
        self
    }
}

impl ParentElement for SearchableListItemElement {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Disableable for SearchableListItemElement {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for SearchableListItemElement {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Sizable for SearchableListItemElement {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for SearchableListItemElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SearchableListItemElement {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let spacing = self.dropdown_style.as_ref().map(|s| s.spacing);
        let row = h_flex()
            .id(self.id)
            .relative()
            .gap_x_1()
            .py_1()
            .px_2()
            .rounded(cx.theme().radius)
            .text_base()
            .text_color(cx.theme().foreground)
            .items_center()
            .justify_between()
            .input_text_size(self.size)
            .list_size(self.size)
            .refine_style(&self.style)
            .when(self.dropdown_style.is_none() && !self.disabled, |this| {
                this.when(!self.selected, |this| {
                    this.hover(|this| this.bg(cx.theme().accent.opacity(0.7)))
                })
            })
            .when(self.dropdown_style.is_none() && self.selected, |this| {
                this.bg(cx.theme().tokens.accent)
            })
            .when_some(self.dropdown_style.as_ref(), |this, style| {
                this.refine_style(&style.item)
                    .when(self.checked, |this| this.refine_style(&style.selected))
                    .when(!self.checked && self.selected, |this| {
                        this.refine_style(&style.hover)
                    })
                    .when(!self.checked && !self.disabled, |this| {
                        let hover = style.hover.clone();
                        this.hover(move |this| this.refine_style(&hover))
                    })
            })
            .when(self.disabled, |this| {
                this.cursor_not_allowed()
                    .text_color(cx.theme().muted_foreground)
            })
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .justify_between()
                    .gap_x_1()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .items_center()
                            .children(self.children),
                    )
                    .when_some(
                        self.check_icon
                            .filter(|_| self.dropdown_style.as_ref().is_none_or(|s| s.show_check)),
                        |this, icon| {
                            this.child(
                                icon.xsmall()
                                    .flex_shrink_0()
                                    .text_color(cx.theme().foreground)
                                    .when(!self.checked, |this| this.invisible()),
                            )
                        },
                    ),
            );
        if let Some(spacing) = spacing {
            gpui::div().pb(spacing).child(row).into_any_element()
        } else {
            row.into_any_element()
        }
    }
}
