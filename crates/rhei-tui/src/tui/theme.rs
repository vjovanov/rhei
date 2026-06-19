//! Shared state→category→glyph→color map: the terminal surface and the browser
//! dashboard reduce a persisted state to the same coarse category so a `blocked`
//! task reads identically everywhere. §FS-rhei-viz.1.1 §FS-rhei-viz-ux.3.2

use ratatui::style::Color;
use rhei_viz_model::Machine;

/// Coarse status category a persisted state reduces to (§FS-rhei-viz.1.1). Rows
/// evaluate top to bottom, first match wins (`Active` is the catch-all); mirrors
/// `rhei_viz::category` against the flattened machine to avoid a validator dep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Category {
    Done,
    Blocked,
    Failed,
    Gate,
    Retired,
    Idle,
    Active,
}

/// Classify a persisted state name against the resolved machine flags.
/// §FS-rhei-viz.1.1
pub(super) fn category(machine: &Machine, state: &str) -> Category {
    let def = machine.states.iter().find(|s| s.name == state);
    if state == "completed" {
        return Category::Done;
    }
    if state == "failed" {
        return Category::Failed;
    }
    if state == "blocked" {
        return Category::Blocked;
    }
    if def.map(|d| d.gating).unwrap_or(false) || state == "human-review" {
        return Category::Gate;
    }
    if def.map(|d| d.terminal).unwrap_or(false) {
        return Category::Retired;
    }
    if state == "cancelled" || state == "archived" {
        return Category::Retired;
    }
    let is_initial = def.map(|d| d.initial).unwrap_or(false);
    if state == "draft" || state == "pending" || is_initial {
        return Category::Idle;
    }
    Category::Active
}

/// The category glyph. Meaning rides the glyph and the state label so the
/// surface is legible under `NO_COLOR`. §FS-rhei-viz-ux.3.3
pub(super) fn category_glyph(category: Category) -> char {
    match category {
        Category::Done => '✓',
        Category::Blocked => '⊘',
        Category::Failed => '✗',
        Category::Gate => '⏸',
        Category::Retired => '⊝',
        Category::Idle => '·',
        Category::Active => '●',
    }
}

/// The render theme. A single flag toggles every colored cell to monochrome,
/// which also selects reduced motion (terminals expose no `prefers-reduced-motion`
/// equivalent). §FS-rhei-viz-ux.3.3 §FS-rhei-viz-ux.4
#[derive(Clone, Copy)]
pub(super) struct Theme {
    /// When false (`NO_COLOR`), every category resolves to the default fg.
    pub(super) color: bool,
}

impl Theme {
    /// Read `NO_COLOR` once at startup. Any non-empty value disables color.
    pub(super) fn from_env() -> Self {
        let color = std::env::var_os("NO_COLOR").map(|v| v.is_empty()).unwrap_or(true);
        Self { color }
    }

    /// Reduced motion follows `NO_COLOR`: the single live spinner stills to a
    /// static dot. §FS-rhei-viz-ux.4
    pub(super) fn reduced_motion(&self) -> bool {
        !self.color
    }

    pub(super) fn category_color(&self, category: Category) -> Color {
        if !self.color {
            return Color::Reset;
        }
        match category {
            Category::Done => Color::Green,
            Category::Blocked => Color::Red,
            Category::Failed => Color::Red,
            Category::Gate => Color::LightCyan,
            Category::Retired => Color::DarkGray,
            Category::Idle => Color::Gray,
            Category::Active => Color::Blue,
        }
    }

    /// The live runtime overlay color (a task assigned to a running slot reads
    /// as `live` even when its persisted state is idle). §FS-rhei-viz.1.1
    pub(super) fn live_color(&self) -> Color {
        if self.color {
            Color::Cyan
        } else {
            Color::Reset
        }
    }

    pub(super) fn accent(&self) -> Color {
        if self.color {
            Color::Cyan
        } else {
            Color::Reset
        }
    }

    pub(super) fn dim(&self) -> Color {
        if self.color {
            Color::DarkGray
        } else {
            Color::Reset
        }
    }
}
