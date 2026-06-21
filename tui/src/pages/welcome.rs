/*
    SPDX-License-Identifier:  AGPL-3.0-or-later
    SPDX-FileCopyrightText:  2025-2026 Shomy
*/
use std::fs;
use std::path::Path;

use anyhow::Result;
use penumbra::da::DAFile;
use ratatui::Frame;
use ratatui::buffer::Buffer;
#[cfg(target_os = "windows")]
use ratatui::crossterm::event::KeyEventKind;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;

use super::LOGO;
use crate::app::{AppCtx, AppPage};
use crate::components::{
    Card, CardRow, DescriptionMenu, DescriptionMenuItem, ExplorerResult, FileExplorer, Stars,
    ThemedWidgetMut,
};
use crate::pages::Page;

type FileVerifier = Box<dyn Fn(&Path, &[u8], &mut AppCtx) -> Result<()> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    SelectDa,
    SelectPreloader,
    EnterDaMode,
    Options,
    Quit,
}

#[derive(Default)]
enum WelcomeState {
    #[default]
    Idle,
    Browsing {
        explorer: FileExplorer,
        callback: Option<FileVerifier>,
    },
}

pub struct WelcomePage {
    state: WelcomeState,
    actions: Vec<MenuAction>,
    menu: DescriptionMenu,
    stars: Stars,
}

impl Default for WelcomePage {
    fn default() -> Self {
        Self::new()
    }
}

impl WelcomePage {
    pub fn new() -> Self {
        let actions = vec![
            MenuAction::SelectDa,
            MenuAction::SelectPreloader,
            MenuAction::EnterDaMode,
            MenuAction::Options,
            MenuAction::Quit,
        ];

        let items = vec![
            DescriptionMenuItem {
                icon: '☾',
                label: "Select DA".into(),
                description: "Select a DA file for entering DA mode".into(),
            },
            DescriptionMenuItem {
                icon: '☾',
                label: "Select Preloader".into(),
                description: "Select a Preloader file, only needed if connecting in BROM".into(),
            },
            DescriptionMenuItem {
                icon: '◈',
                label: "Enter DA Mode".into(),
                description: "Flash, unlock, and manage device".into(),
            },
            DescriptionMenuItem {
                icon: '',
                label: "Options".into(),
                description: "Change Antumbra settings".into(),
            },
            DescriptionMenuItem {
                icon: '⏻',
                label: "Quit".into(),
                description: "Exit Antumbra".into(),
            },
        ];

        Self {
            state: WelcomeState::Idle,
            actions,
            menu: DescriptionMenu::new(items),
            stars: Stars::new(3.0),
        }
    }

    fn open_da_loader(&mut self) {
        match FileExplorer::new("Select DA File") {
            Ok(explorer) => {
                let callback: FileVerifier =
                    Box::new(|path, data, ctx| match DAFile::parse_da(data) {
                        Ok(da_file) => {
                            ctx.set_loader(path.to_path_buf(), da_file);
                            Ok(())
                        }
                        Err(e) => Err(anyhow::anyhow!(e.to_string())),
                    });

                self.state = WelcomeState::Browsing {
                    explorer: explorer.extensions(&["bin"]),
                    callback: Some(callback),
                };
            }
            Err(err) => {
                eprintln!("Failed to launch file explorer: {err}");
            }
        }
    }

    fn open_preloader(&mut self) {
        match FileExplorer::new("Select Preloader File") {
            Ok(explorer) => {
                let callback: FileVerifier = Box::new(|path, data, ctx| {
                    ctx.set_preloader(path.to_path_buf(), data.to_vec());
                    Ok(())
                });

                self.state = WelcomeState::Browsing {
                    explorer: explorer.extensions(&["bin"]),
                    callback: Some(callback),
                };
            }
            Err(err) => {
                eprintln!("Failed to launch file explorer: {err}");
            }
        }
    }

    fn current_action(&self) -> Option<MenuAction> {
        self.actions.get(self.menu.selected_index()).copied()
    }

    fn render_status_cards(&self, area: Rect, buf: &mut Buffer, ctx: &AppCtx) {
        let card_width = 32u16;
        let da_value =
            if ctx.loader().is_some() { ctx.loader_name() } else { "Not selected".to_string() };
        let pl_value = if ctx.preloader().is_some() {
            ctx.preloader_name()
        } else {
            "Not selected".to_string()
        };
        let style_border = Style::default().fg(ctx.theme.muted);

        let cards = vec![
            Card::new("☽ DA", &da_value, card_width, style_border),
            Card::new("⚡ PL", &pl_value, card_width, style_border),
        ];

        CardRow::new(cards, 2).render(buf, area.x, area.width, area.y);
    }
}

#[async_trait::async_trait]
impl Page for WelcomePage {
    fn render(&mut self, f: &mut Frame, ctx: &mut AppCtx) {
        let area = f.area();

        self.stars.tick();
        self.stars.render(area, f.buffer_mut(), &ctx.theme);

        // Layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),  // Top stars
                Constraint::Length(12), // Logo
                Constraint::Min(12),    // Menu
                Constraint::Length(3),  // Status cards
                Constraint::Length(1),  // Footer
            ])
            .split(area);

        // Logo
        let logo = Paragraph::new(LOGO)
            .alignment(Alignment::Center)
            .style(Style::default().fg(ctx.theme.accent));
        f.render_widget(logo, chunks[1]);

        let menu_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Length(50), Constraint::Fill(1)])
            .split(chunks[2]);

        self.menu.render(menu_layout[1], f.buffer_mut(), &ctx.theme);

        self.render_status_cards(chunks[3], f.buffer_mut(), ctx);

        let footer = Paragraph::new("[↑↓] Navigate    [Enter] Select    [Esc] Back")
            .alignment(Alignment::Center)
            .style(Style::default().fg(ctx.theme.muted));
        f.render_widget(footer, chunks[4]);

        if let WelcomeState::Browsing { explorer, callback: _ } = &mut self.state {
            explorer.render_modal(area, f.buffer_mut(), &ctx.theme);
        }
    }

    async fn handle_input(&mut self, ctx: &mut AppCtx, key: KeyEvent) {
        #[cfg(target_os = "windows")]
        if key.kind != KeyEventKind::Press {
            return;
        }

        match &mut self.state {
            WelcomeState::Browsing { explorer, callback } => match explorer.handle_key(key) {
                ExplorerResult::Selected(path) => match fs::read(&path) {
                    Ok(data) => {
                        if let Some(cb) = callback {
                            if let Err(e) = cb(&path, &data, ctx) {
                                error_dialog!(ctx, e.to_string());
                            }
                        } else {
                            match DAFile::parse_da(&data) {
                                Ok(da_file) => ctx.set_loader(path.to_path_buf(), da_file),
                                Err(e) => error_dialog!(ctx, e.to_string()),
                            }
                        }
                        self.state = WelcomeState::Idle;
                    }
                    Err(e) => error_dialog!(ctx, e.to_string()),
                },
                ExplorerResult::Cancelled => self.state = WelcomeState::Idle,
                ExplorerResult::Pending => {}
            },

            WelcomeState::Idle => match key.code {
                KeyCode::Up => self.menu.previous(),
                KeyCode::Down => self.menu.next(),
                KeyCode::Enter => match self.current_action() {
                    Some(MenuAction::SelectDa) => self.open_da_loader(),
                    Some(MenuAction::SelectPreloader) => self.open_preloader(),
                    Some(MenuAction::EnterDaMode) => ctx.change_page(AppPage::DevicePage),
                    Some(MenuAction::Options) => ctx.change_page(AppPage::Options),
                    Some(MenuAction::Quit) => ctx.quit(),
                    None => {}
                },
                _ => {}
            },
        }
    }
}
