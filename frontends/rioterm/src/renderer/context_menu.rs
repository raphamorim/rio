// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::context::next_rich_text_id;
use crate::renderer::utils::add_span_with_fallback;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};

const MENU_WIDTH: f32 = 220.0;
const ITEM_HEIGHT: f32 = 32.0;
const MENU_PADDING_V: f32 = 6.0;
const ORDER: u8 = 21;
const BG_COLOR: [f32; 4] = [0.13, 0.13, 0.13, 0.97];
const HOVER_BG_COLOR: [f32; 4] = [0.25, 0.25, 0.25, 1.0];
const TEXT_COLOR: [f32; 4] = [0.92, 0.92, 0.92, 1.0];
const DISABLED_COLOR: [f32; 4] = [0.50, 0.50, 0.50, 1.0];
const DEPTH_BACKDROP: f32 = -0.99;
const DEPTH_BG: f32 = -0.98;
const DEPTH_ELEMENT: f32 = -0.97;
const FONT_SIZE: f32 = 13.0;
const PADDING_X: f32 = 12.0;

/// Actions available in the context menu.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextMenuAction {
    Copy,
    Paste,
    SelectAll,
    SplitRight,
    SplitDown,
    NewTab,
    CloseTab,
}

struct ContextMenuItem {
    rotulo: &'static str,
    acao: ContextMenuAction,
    habilitado: bool,
}

/// Context menu overlay rendered via sugarloaf.
pub struct ContextMenu {
    esta_visivel: bool,
    ancora_x: f32,
    ancora_y: f32,
    item_em_foco: Option<usize>,
    itens: Vec<ContextMenuItem>,
    ids_texto: Vec<usize>,
}

impl ContextMenu {
    pub fn new() -> Self {
        ContextMenu {
            esta_visivel: false,
            ancora_x: 0.0,
            ancora_y: 0.0,
            item_em_foco: None,
            itens: Vec::new(),
            ids_texto: Vec::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.esta_visivel
    }

    pub fn show(
        &mut self,
        x: f32,
        y: f32,
        sugarloaf: &mut Sugarloaf,
        has_selection: bool,
    ) {
        self.ancora_x = x;
        self.ancora_y = y;
        self.item_em_foco = None;
        self.esta_visivel = true;

        self.itens = vec![
            ContextMenuItem {
                rotulo: "Copiar",
                acao: ContextMenuAction::Copy,
                habilitado: has_selection,
            },
            ContextMenuItem {
                rotulo: "Colar",
                acao: ContextMenuAction::Paste,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Selecionar tudo",
                acao: ContextMenuAction::SelectAll,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Dividir direita",
                acao: ContextMenuAction::SplitRight,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Dividir abaixo",
                acao: ContextMenuAction::SplitDown,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Nova aba",
                acao: ContextMenuAction::NewTab,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Fechar aba",
                acao: ContextMenuAction::CloseTab,
                habilitado: true,
            },
        ];

        self.garantir_ids_texto(sugarloaf);
    }

    pub fn hide(&mut self) {
        self.esta_visivel = false;
        self.item_em_foco = None;
    }

    /// Hit-test: `Err(())` = outside menu, `Ok(None)` = inside but no item, `Ok(Some(i))` = item index hit.
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        window_height: f32,
        scale_factor: f32,
    ) -> Result<Option<usize>, ()> {
        if !self.esta_visivel || self.itens.is_empty() {
            return Err(());
        }

        let (mx, my, mw, mh) = self
            .retangulo_menu(window_width / scale_factor, window_height / scale_factor);

        let lx = mouse_x / scale_factor;
        let ly = mouse_y / scale_factor;

        if lx < mx || lx > mx + mw || ly < my || ly > my + mh {
            return Err(());
        }

        let content_y = my + MENU_PADDING_V;
        for (i, _item) in self.itens.iter().enumerate() {
            let item_top = content_y + i as f32 * ITEM_HEIGHT;
            let item_bottom = item_top + ITEM_HEIGHT;
            if ly >= item_top && ly < item_bottom {
                return Ok(Some(i));
            }
        }

        Ok(None)
    }

    pub fn get_action(&self, index: usize) -> Option<ContextMenuAction> {
        self.itens.get(index).and_then(|item| {
            if item.habilitado {
                Some(item.acao.clone())
            } else {
                None
            }
        })
    }

    #[cfg(test)]
    pub fn set_hovered(&mut self, index: Option<usize>) {
        self.item_em_foco = index;
    }

    /// Atualiza item em foco com base na posição do mouse; retorna `true` se o estado mudou.
    pub fn hover(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        window_height: f32,
        scale_factor: f32,
    ) -> bool {
        if !self.esta_visivel || self.itens.is_empty() {
            return false;
        }
        let novo = match self.hit_test(
            mouse_x,
            mouse_y,
            window_width,
            window_height,
            scale_factor,
        ) {
            Ok(Some(i)) => Some(i),
            _ => None,
        };
        if self.item_em_foco != novo {
            self.item_em_foco = novo;
            true
        } else {
            false
        }
    }

    pub fn render(&mut self, sugarloaf: &mut Sugarloaf, dimensions: (f32, f32, f32)) {
        if !self.esta_visivel {
            self.ocultar_todos_ids_texto(sugarloaf);
            return;
        }

        let (window_width, window_height, scale_factor) = dimensions;
        let lw = window_width / scale_factor;
        let lh = window_height / scale_factor;

        let (mx, my, mw, mh) = self.retangulo_menu(lw, lh);

        // Backdrop (click-outside catcher layer)
        sugarloaf.rect(
            None,
            0.0,
            0.0,
            lw,
            lh,
            [0.0, 0.0, 0.0, 0.0],
            DEPTH_BACKDROP,
            ORDER,
        );

        // Background
        sugarloaf.rounded_rect(None, mx, my, mw, mh, BG_COLOR, DEPTH_BG, 6.0, ORDER);

        self.garantir_ids_texto(sugarloaf);

        let content_y = my + MENU_PADDING_V;
        for (i, item) in self.itens.iter().enumerate() {
            let item_y = content_y + i as f32 * ITEM_HEIGHT;

            // Hover highlight
            if self.item_em_foco == Some(i) {
                sugarloaf.rounded_rect(
                    None,
                    mx + 4.0,
                    item_y + 2.0,
                    mw - 8.0,
                    ITEM_HEIGHT - 4.0,
                    HOVER_BG_COLOR,
                    DEPTH_ELEMENT,
                    4.0,
                    ORDER,
                );
            }

            if let Some(&id) = self.ids_texto.get(i) {
                let cor = if item.habilitado {
                    TEXT_COLOR
                } else {
                    DISABLED_COLOR
                };
                let estilo = SpanStyle {
                    color: cor,
                    ..SpanStyle::default()
                };
                sugarloaf.content().sel(id).clear().new_line();
                add_span_with_fallback(sugarloaf, item.rotulo, estilo);
                sugarloaf.content().build();

                let texto_y = item_y + (ITEM_HEIGHT - FONT_SIZE) / 2.0;
                sugarloaf.set_position(id, mx + PADDING_X, texto_y);
                sugarloaf.set_visibility(id, true);
            }
        }

        // Hide unused text IDs
        for i in self.itens.len()..self.ids_texto.len() {
            sugarloaf.set_visibility(self.ids_texto[i], false);
        }
    }

    fn retangulo_menu(
        &self,
        largura_janela: f32,
        altura_janela: f32,
    ) -> (f32, f32, f32, f32) {
        let n = self.itens.len() as f32;
        let altura = MENU_PADDING_V * 2.0 + n * ITEM_HEIGHT;

        let x = self.ancora_x.min(largura_janela - MENU_WIDTH).max(0.0);
        let y = self.ancora_y.min(altura_janela - altura).max(0.0);

        (x, y, MENU_WIDTH, altura)
    }

    fn garantir_ids_texto(&mut self, sugarloaf: &mut Sugarloaf) {
        while self.ids_texto.len() < self.itens.len() {
            let id = next_rich_text_id();
            let _ = sugarloaf.text(Some(id));
            sugarloaf.set_use_grid_cell_size(id, false);
            sugarloaf.set_text_font_size(&id, FONT_SIZE);
            sugarloaf.set_order(id, ORDER);
            self.ids_texto.push(id);
        }
    }

    fn ocultar_todos_ids_texto(&self, sugarloaf: &mut Sugarloaf) {
        for &id in &self.ids_texto {
            sugarloaf.set_visibility(id, false);
        }
    }
}

impl Default for ContextMenu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn menu_inicia_invisivel() {
        let menu = ContextMenu::new();
        assert!(!menu.is_enabled());
    }

    #[test]
    fn hide_desabilita_menu() {
        let mut menu = ContextMenu::new();
        menu.esta_visivel = true;
        menu.hide();
        assert!(!menu.is_enabled());
    }

    #[test]
    fn hide_limpa_foco() {
        let mut menu = ContextMenu::new();
        menu.esta_visivel = true;
        menu.item_em_foco = Some(2);
        menu.hide();
        assert_eq!(menu.item_em_foco, None);
    }

    #[test]
    fn set_hovered_atualiza_foco() {
        let mut menu = ContextMenu::new();
        menu.esta_visivel = true;
        menu.ancora_x = 0.0;
        menu.ancora_y = 0.0;
        menu.itens = vec![
            ContextMenuItem {
                rotulo: "a",
                acao: ContextMenuAction::Copy,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "b",
                acao: ContextMenuAction::Paste,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "c",
                acao: ContextMenuAction::SelectAll,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "d",
                acao: ContextMenuAction::SplitRight,
                habilitado: true,
            },
        ];
        // hover over 4th item (index 3)
        let y = MENU_PADDING_V + 3.0 * ITEM_HEIGHT + ITEM_HEIGHT / 2.0;
        menu.hover(50.0, y, 1200.0, 800.0, 1.0);
        assert_eq!(menu.item_em_foco, Some(3));
        // hover outside
        menu.hover(5000.0, 5000.0, 1200.0, 800.0, 1.0);
        assert_eq!(menu.item_em_foco, None);
    }

    #[test]
    fn get_action_para_item_habilitado() {
        let mut menu = ContextMenu::new();
        menu.itens = vec![ContextMenuItem {
            rotulo: "Colar",
            acao: ContextMenuAction::Paste,
            habilitado: true,
        }];
        assert_eq!(menu.get_action(0), Some(ContextMenuAction::Paste));
    }

    #[test]
    fn get_action_para_item_desabilitado_retorna_none() {
        let mut menu = ContextMenu::new();
        menu.itens = vec![ContextMenuItem {
            rotulo: "Copiar",
            acao: ContextMenuAction::Copy,
            habilitado: false,
        }];
        assert_eq!(menu.get_action(0), None);
    }

    #[test]
    fn get_action_para_indice_invalido_retorna_none() {
        let menu = ContextMenu::new();
        assert_eq!(menu.get_action(99), None);
    }

    #[test]
    fn hit_test_fora_retorna_err() {
        let mut menu = ContextMenu::new();
        menu.esta_visivel = true;
        menu.ancora_x = 100.0;
        menu.ancora_y = 100.0;
        menu.itens = vec![ContextMenuItem {
            rotulo: "Colar",
            acao: ContextMenuAction::Paste,
            habilitado: true,
        }];
        assert!(menu.hit_test(0.0, 0.0, 1200.0, 800.0, 1.0).is_err());
    }

    #[test]
    fn hit_test_primeiro_item() {
        let mut menu = ContextMenu::new();
        menu.esta_visivel = true;
        menu.ancora_x = 100.0;
        menu.ancora_y = 100.0;
        menu.itens = vec![
            ContextMenuItem {
                rotulo: "Copiar",
                acao: ContextMenuAction::Copy,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Colar",
                acao: ContextMenuAction::Paste,
                habilitado: true,
            },
        ];
        let y = 100.0 + MENU_PADDING_V + ITEM_HEIGHT / 2.0;
        let result = menu.hit_test(150.0, y, 1200.0, 800.0, 1.0);
        assert_eq!(result, Ok(Some(0)));
    }

    #[test]
    fn hit_test_segundo_item() {
        let mut menu = ContextMenu::new();
        menu.esta_visivel = true;
        menu.ancora_x = 100.0;
        menu.ancora_y = 100.0;
        menu.itens = vec![
            ContextMenuItem {
                rotulo: "Copiar",
                acao: ContextMenuAction::Copy,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Colar",
                acao: ContextMenuAction::Paste,
                habilitado: true,
            },
        ];
        let y = 100.0 + MENU_PADDING_V + ITEM_HEIGHT + ITEM_HEIGHT / 2.0;
        let result = menu.hit_test(150.0, y, 1200.0, 800.0, 1.0);
        assert_eq!(result, Ok(Some(1)));
    }

    #[test]
    fn hit_test_com_escala() {
        let mut menu = ContextMenu::new();
        menu.esta_visivel = true;
        menu.ancora_x = 100.0;
        menu.ancora_y = 100.0;
        menu.itens = vec![ContextMenuItem {
            rotulo: "Colar",
            acao: ContextMenuAction::Paste,
            habilitado: true,
        }];
        // Com scale_factor=2.0, coordenadas físicas são dobradas
        let phys_x = 150.0 * 2.0;
        let phys_y = (100.0 + MENU_PADDING_V + ITEM_HEIGHT / 2.0) * 2.0;
        let result = menu.hit_test(phys_x, phys_y, 1200.0 * 2.0, 800.0 * 2.0, 2.0);
        assert_eq!(result, Ok(Some(0)));
    }

    #[test]
    fn menu_ajusta_posicao_ao_transbordar_direita() {
        let mut menu = ContextMenu::new();
        menu.ancora_x = 1190.0;
        menu.ancora_y = 100.0;
        menu.itens = vec![ContextMenuItem {
            rotulo: "Colar",
            acao: ContextMenuAction::Paste,
            habilitado: true,
        }];
        let (x, _, w, _) = menu.retangulo_menu(1200.0, 800.0);
        assert!(x + w <= 1200.0);
    }

    #[test]
    fn menu_ajusta_posicao_ao_transbordar_baixo() {
        let mut menu = ContextMenu::new();
        menu.ancora_x = 100.0;
        menu.ancora_y = 780.0;
        menu.itens = vec![ContextMenuItem {
            rotulo: "Colar",
            acao: ContextMenuAction::Paste,
            habilitado: true,
        }];
        let (_, y, _, h) = menu.retangulo_menu(1200.0, 800.0);
        assert!(y + h <= 800.0);
    }

    #[test]
    fn menu_sete_itens_por_padrao() {
        let mut menu = ContextMenu::new();
        menu.itens = vec![
            ContextMenuItem {
                rotulo: "Copiar",
                acao: ContextMenuAction::Copy,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Colar",
                acao: ContextMenuAction::Paste,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Selecionar tudo",
                acao: ContextMenuAction::SelectAll,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Dividir direita",
                acao: ContextMenuAction::SplitRight,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Dividir abaixo",
                acao: ContextMenuAction::SplitDown,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Nova aba",
                acao: ContextMenuAction::NewTab,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Fechar aba",
                acao: ContextMenuAction::CloseTab,
                habilitado: true,
            },
        ];
        assert_eq!(menu.itens.len(), 7);
    }

    #[test]
    fn copiar_desabilitado_sem_selecao() {
        let mut menu = ContextMenu::new();
        menu.itens = vec![ContextMenuItem {
            rotulo: "Copiar",
            acao: ContextMenuAction::Copy,
            habilitado: false,
        }];
        assert_eq!(menu.get_action(0), None);
    }

    #[test]
    fn copiar_habilitado_com_selecao() {
        let mut menu = ContextMenu::new();
        menu.itens = vec![ContextMenuItem {
            rotulo: "Copiar",
            acao: ContextMenuAction::Copy,
            habilitado: true,
        }];
        assert_eq!(menu.get_action(0), Some(ContextMenuAction::Copy));
    }

    #[test]
    fn retangulo_menu_altura_cobre_todos_itens() {
        let mut menu = ContextMenu::new();
        menu.itens = vec![
            ContextMenuItem {
                rotulo: "a",
                acao: ContextMenuAction::Copy,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "b",
                acao: ContextMenuAction::Paste,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "c",
                acao: ContextMenuAction::SelectAll,
                habilitado: true,
            },
        ];
        let (_, _, _, h) = menu.retangulo_menu(1200.0, 800.0);
        let esperado = MENU_PADDING_V * 2.0 + 3.0 * ITEM_HEIGHT;
        assert!((h - esperado).abs() < f32::EPSILON);
    }

    fn menu_com_sete_itens() -> ContextMenu {
        let mut menu = ContextMenu::new();
        menu.esta_visivel = true;
        menu.ancora_x = 100.0;
        menu.ancora_y = 100.0;
        menu.itens = vec![
            ContextMenuItem {
                rotulo: "Copiar",
                acao: ContextMenuAction::Copy,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Colar",
                acao: ContextMenuAction::Paste,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Selecionar tudo",
                acao: ContextMenuAction::SelectAll,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Dividir direita",
                acao: ContextMenuAction::SplitRight,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Dividir abaixo",
                acao: ContextMenuAction::SplitDown,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Nova aba",
                acao: ContextMenuAction::NewTab,
                habilitado: true,
            },
            ContextMenuItem {
                rotulo: "Fechar aba",
                acao: ContextMenuAction::CloseTab,
                habilitado: true,
            },
        ];
        menu
    }

    #[test]
    fn teste_context_menu_new_inicia_oculto() {
        let menu = ContextMenu::new();
        assert!(!menu.is_enabled());
        assert_eq!(menu.item_em_foco, None);
        assert!(menu.itens.is_empty());
    }

    #[test]
    fn teste_context_menu_show_armazena_posicao() {
        let mut menu = ContextMenu::new();
        menu.ancora_x = 200.0;
        menu.ancora_y = 300.0;
        menu.esta_visivel = true;
        assert!(menu.is_enabled());
        assert!((menu.ancora_x - 200.0).abs() < f32::EPSILON);
        assert!((menu.ancora_y - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn teste_context_menu_hide_oculta() {
        let mut menu = ContextMenu::new();
        menu.esta_visivel = true;
        menu.item_em_foco = Some(1);
        menu.hide();
        assert!(!menu.is_enabled());
        assert_eq!(menu.item_em_foco, None);
    }

    #[test]
    fn teste_hit_test_retorna_item_correto_para_pixel() {
        let menu = menu_com_sete_itens();
        // Primeiro item: centro vertical do item 0
        let y_item0 = 100.0 + MENU_PADDING_V + ITEM_HEIGHT / 2.0;
        assert_eq!(
            menu.hit_test(150.0, y_item0, 1200.0, 800.0, 1.0),
            Ok(Some(0))
        );
        // Segundo item
        let y_item1 = 100.0 + MENU_PADDING_V + ITEM_HEIGHT + ITEM_HEIGHT / 2.0;
        assert_eq!(
            menu.hit_test(150.0, y_item1, 1200.0, 800.0, 1.0),
            Ok(Some(1))
        );
        // Sétimo item
        let y_item6 = 100.0 + MENU_PADDING_V + 6.0 * ITEM_HEIGHT + ITEM_HEIGHT / 2.0;
        assert_eq!(
            menu.hit_test(150.0, y_item6, 1200.0, 800.0, 1.0),
            Ok(Some(6))
        );
    }

    #[test]
    fn teste_hit_test_retorna_none_fora_do_menu() {
        let menu = menu_com_sete_itens();
        // À esquerda do menu
        assert!(menu.hit_test(0.0, 120.0, 1200.0, 800.0, 1.0).is_err());
        // Acima do menu
        assert!(menu.hit_test(150.0, 50.0, 1200.0, 800.0, 1.0).is_err());
        // À direita do menu (ancora_x=100 + MENU_WIDTH=220 → borda direita 320)
        assert!(menu.hit_test(400.0, 120.0, 1200.0, 800.0, 1.0).is_err());
        // Abaixo do menu
        let (_, my, _, mh) = menu.retangulo_menu(1200.0, 800.0);
        assert!(menu
            .hit_test(150.0, my + mh + 10.0, 1200.0, 800.0, 1.0)
            .is_err());
    }

    #[test]
    fn teste_navegacao_teclado_move_selecao() {
        let mut menu = menu_com_sete_itens();
        // Estado inicial: sem foco
        assert_eq!(menu.item_em_foco, None);
        // set_hovered simula navegação por teclado
        menu.set_hovered(Some(0));
        assert_eq!(menu.get_action(0), Some(ContextMenuAction::Copy));
        menu.set_hovered(Some(1));
        assert_eq!(menu.get_action(1), Some(ContextMenuAction::Paste));
        menu.set_hovered(Some(2));
        assert_eq!(menu.get_action(2), Some(ContextMenuAction::SelectAll));
        menu.set_hovered(Some(1));
        assert_eq!(menu.get_action(1), Some(ContextMenuAction::Paste));
    }

    #[test]
    fn teste_activate_selected_retorna_action_correta() {
        let menu = menu_com_sete_itens();
        assert_eq!(menu.get_action(0), Some(ContextMenuAction::Copy));
        assert_eq!(menu.get_action(1), Some(ContextMenuAction::Paste));
        assert_eq!(menu.get_action(2), Some(ContextMenuAction::SelectAll));
        assert_eq!(menu.get_action(3), Some(ContextMenuAction::SplitRight));
        assert_eq!(menu.get_action(4), Some(ContextMenuAction::SplitDown));
        assert_eq!(menu.get_action(5), Some(ContextMenuAction::NewTab));
        assert_eq!(menu.get_action(6), Some(ContextMenuAction::CloseTab));
        // Índice fora dos limites
        assert_eq!(menu.get_action(7), None);
    }

    #[test]
    fn teste_hit_test_com_scale_factor_2() {
        let menu = menu_com_sete_itens();
        // scale_factor=2.0: coordenadas físicas dobradas, lógicas iguais
        let phys_x = 150.0 * 2.0;
        let phys_y = (100.0 + MENU_PADDING_V + ITEM_HEIGHT / 2.0) * 2.0;
        assert_eq!(
            menu.hit_test(phys_x, phys_y, 1200.0 * 2.0, 800.0 * 2.0, 2.0),
            Ok(Some(0))
        );
    }

    #[test]
    fn teste_hit_test_menu_invisivel_retorna_err() {
        let mut menu = menu_com_sete_itens();
        menu.esta_visivel = false;
        let y = 100.0 + MENU_PADDING_V + ITEM_HEIGHT / 2.0;
        assert!(menu.hit_test(150.0, y, 1200.0, 800.0, 1.0).is_err());
    }

    #[test]
    fn teste_retangulo_menu_clamped_na_borda_direita() {
        let mut menu = ContextMenu::new();
        menu.ancora_x = 1190.0;
        menu.ancora_y = 100.0;
        menu.itens = vec![ContextMenuItem {
            rotulo: "a",
            acao: ContextMenuAction::Copy,
            habilitado: true,
        }];
        let (x, _, w, _) = menu.retangulo_menu(1200.0, 800.0);
        assert!(x + w <= 1200.0, "menu não deve ultrapassar a borda direita");
    }

    #[test]
    fn teste_retangulo_menu_clamped_na_borda_inferior() {
        let mut menu = ContextMenu::new();
        menu.ancora_x = 100.0;
        menu.ancora_y = 790.0;
        menu.itens = vec![ContextMenuItem {
            rotulo: "a",
            acao: ContextMenuAction::Copy,
            habilitado: true,
        }];
        let (_, y, _, h) = menu.retangulo_menu(1200.0, 800.0);
        assert!(y + h <= 800.0, "menu não deve ultrapassar a borda inferior");
    }

    /// Grade 8×8 de coordenadas dentro do menu — cada célula deve retornar Ok(Some(_)).
    #[test]
    fn teste_hit_test_grade_dentro_do_menu() {
        let menu = menu_com_sete_itens();
        let (mx, my, mw, _) = menu.retangulo_menu(1200.0, 800.0);

        // 8 colunas uniformes dentro da largura do menu
        let offsets_x: [f32; 8] = [
            mw * 0.06,
            mw * 0.18,
            mw * 0.30,
            mw * 0.42,
            mw * 0.54,
            mw * 0.66,
            mw * 0.78,
            mw * 0.90,
        ];
        // 7 linhas — centro vertical de cada item
        for item_idx in 0usize..7 {
            let py =
                my + MENU_PADDING_V + item_idx as f32 * ITEM_HEIGHT + ITEM_HEIGHT / 2.0;
            for &ox in &offsets_x {
                let px = mx + ox;
                let resultado = menu.hit_test(px, py, 1200.0, 800.0, 1.0);
                assert!(
                    resultado.is_ok(),
                    "esperado Ok para item={item_idx} px={px:.1} py={py:.1}, obtido {resultado:?}"
                );
            }
        }
    }

    /// Grade 8×8 de coordenadas FORA do menu — cada célula deve retornar Err(()).
    #[test]
    fn teste_hit_test_grade_fora_do_menu() {
        let menu = menu_com_sete_itens();
        let (mx, my, mw, mh) = menu.retangulo_menu(1200.0, 800.0);

        // Pontos fora: à esquerda, acima, à direita e abaixo
        // 8 variações de offset para cobrir a grade
        let offsets: [f32; 8] = [5.0, 15.0, 25.0, 35.0, 45.0, 55.0, 65.0, 75.0];

        for &d in &offsets {
            // À esquerda (x < mx)
            let fora_esq = menu.hit_test(mx - d, my + mh / 2.0, 1200.0, 800.0, 1.0);
            assert!(
                fora_esq.is_err(),
                "esquerda d={d}: esperado Err, obtido {fora_esq:?}"
            );

            // Acima (y < my)
            let fora_cima = menu.hit_test(mx + mw / 2.0, my - d, 1200.0, 800.0, 1.0);
            assert!(
                fora_cima.is_err(),
                "acima d={d}: esperado Err, obtido {fora_cima:?}"
            );

            // À direita (x > mx + mw)
            let fora_dir = menu.hit_test(mx + mw + d, my + mh / 2.0, 1200.0, 800.0, 1.0);
            assert!(
                fora_dir.is_err(),
                "direita d={d}: esperado Err, obtido {fora_dir:?}"
            );

            // Abaixo (y > my + mh)
            let fora_baixo =
                menu.hit_test(mx + mw / 2.0, my + mh + d, 1200.0, 800.0, 1.0);
            assert!(
                fora_baixo.is_err(),
                "abaixo d={d}: esperado Err, obtido {fora_baixo:?}"
            );
        }
    }
}
