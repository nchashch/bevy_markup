//! Language selector: one button per [`Locales`] entry. Clicking a button
//! makes its bundle the [`ActiveLocale`]; every `HtmlView` re-localizes.

use bevy::prelude::*;

use super::NineSliceFrame;
use crate::assets::l10n::{ActiveLocale, Locales};
use crate::consts::{BODY_COLOR, BODY_FONT_PATH, FRAME_PATH, HEADER_COLOR, HEADER_FONT_PATH};

const BUTTON_IDLE: Color = Color::srgb_u8(40, 40, 46);
const BUTTON_HOVER: Color = Color::srgb_u8(64, 64, 72);
const ACTIVE_TEXT: Color = Color::WHITE;

/// A selector button for `Locales.0[index]`.
#[derive(Component)]
pub(super) struct LocaleButton(usize);

/// PostStartup: needs [`Locales`], inserted by `L10nPlugin` during Startup.
pub(super) fn spawn(mut commands: Commands, asset_server: Res<AssetServer>, locales: Res<Locales>) {
    let body_font = TextFont::default()
        .with_font(asset_server.load(BODY_FONT_PATH))
        .with_font_size(18.0);

    commands
        .spawn((
            NineSliceFrame(asset_server.load(FRAME_PATH)),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(232.0),
                width: Val::Px(420.0),
                padding: UiRect::all(Val::Px(28.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                ..default()
            },
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Language"),
                TextFont::default()
                    .with_font(asset_server.load(HEADER_FONT_PATH))
                    .with_font_size(24.0),
                TextColor(HEADER_COLOR),
            ));
            panel
                .spawn(Node {
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    for (index, locale) in locales.0.iter().enumerate() {
                        row.spawn((
                            Button,
                            LocaleButton(index),
                            Node {
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(BUTTON_IDLE),
                            children![(
                                Text::new(locale.name),
                                body_font.clone(),
                                TextColor(BODY_COLOR),
                                // Clicks on the label count as clicks on the button.
                                Pickable::IGNORE,
                            )],
                        ))
                        .observe(select_locale);
                    }
                });
        });
}

fn select_locale(
    click: On<Pointer<Click>>,
    buttons: Query<&LocaleButton>,
    locales: Res<Locales>,
    mut active: ResMut<ActiveLocale>,
) {
    let Ok(LocaleButton(index)) = buttons.get(click.entity) else {
        return;
    };
    let bundle = &locales.0[*index].bundle;
    if active.0 != *bundle {
        active.0 = bundle.clone();
        info!("locale -> {}", locales.0[*index].id);
    }
}

/// Active locale: red button, white label. Others: dark, lighter on hover.
pub(super) fn style_locale_buttons(
    active: Res<ActiveLocale>,
    locales: Res<Locales>,
    mut buttons: Query<(
        Ref<Interaction>,
        &LocaleButton,
        &mut BackgroundColor,
        &Children,
    )>,
    mut labels: Query<&mut TextColor>,
) {
    for (interaction, LocaleButton(index), mut background, children) in &mut buttons {
        if !active.is_changed() && !interaction.is_changed() {
            continue;
        }
        let is_active = locales.0[*index].bundle == active.0;
        let (bg, fg) = if is_active {
            (HEADER_COLOR, ACTIVE_TEXT)
        } else if *interaction != Interaction::None {
            (BUTTON_HOVER, BODY_COLOR)
        } else {
            (BUTTON_IDLE, BODY_COLOR)
        };
        background.0 = bg;
        let mut labels = labels.iter_many_mut(children);
        while let Some(mut label) = labels.fetch_next() {
            label.0 = fg;
        }
    }
}
