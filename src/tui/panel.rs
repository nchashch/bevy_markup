use bevy::prelude::*;
use bevy_tui_texture::prelude::*;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, Gauge, Paragraph};

/// Marker for the panel terminal entity.
#[derive(Component)]
pub struct TuiPanel;

/// Ratatui code for the panel. Runs every frame; unchanged cells cost nothing.
pub(super) fn draw(mut panels: Query<&mut Tui, With<TuiPanel>>, time: Res<Time>) {
    // `Tui` doesn't exist until the font asset has loaded.
    let Ok(mut term) = panels.single_mut() else {
        return;
    };
    let elapsed = time.elapsed_secs();

    term.draw(|frame| {
        let outer = Block::bordered()
            .title(" p23 ")
            .border_style(Style::default().fg(Color::LightCyan));
        let inner = outer.inner(frame.area());
        frame.render_widget(outer, frame.area());

        let [text_area, gauge_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

        frame.render_widget(
            Paragraph::new(format!("Hello from ratatui\nt = {elapsed:.1}s"))
                .alignment(Alignment::Center)
                .fg(Color::White)
                .bold(),
            text_area,
        );

        let ratio = f64::from((elapsed.sin() + 1.0) / 2.0);
        frame.render_widget(
            Gauge::default()
                .ratio(ratio)
                .gauge_style(Style::default().fg(Color::Magenta)),
            gauge_area,
        );
    });
}
