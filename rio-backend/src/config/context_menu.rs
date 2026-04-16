//! Configuração do menu de contexto exibido ao clicar com o botão direito.
//!
//! As opções são lidas a partir do bloco `[context-menu]` do arquivo de
//! configuração TOML do Rio. Todos os campos possuem valores padrão e podem
//! ser omitidos.
//!
//! ## Exemplo de configuração (`config.toml`)
//!
//! ```toml
//! [context-menu]
//! enabled           = true
//! background-color  = "#1e1e2e"
//! foreground-color  = "#cdd6f4"
//! selection-color   = "#89b4fa"
//! divider-color     = "#45475a"
//! font-size         = 14.0
//! border-radius     = 6.0
//! padding           = 8.0
//! ```

use crate::config::colors::{deserialize_to_arr, ColorArray};
use serde::{Deserialize, Serialize};

#[inline]
fn padrao_habilitado() -> bool {
    true
}

#[inline]
fn padrao_cor_fundo() -> ColorArray {
    // Cinza escuro neutro, adequado para temas claros e escuros
    [0.12, 0.12, 0.12, 1.0]
}

#[inline]
fn padrao_cor_texto() -> ColorArray {
    // Branco levemente suavizado
    [0.90, 0.90, 0.90, 1.0]
}

#[inline]
fn padrao_cor_hover() -> ColorArray {
    // Azul padrão de seleção
    [0.20, 0.47, 0.82, 1.0]
}

#[inline]
fn padrao_cor_divisor() -> ColorArray {
    // Cinza médio para linha separadora
    [0.30, 0.30, 0.30, 1.0]
}

#[inline]
fn padrao_tamanho_fonte() -> f32 {
    14.0
}

#[inline]
fn padrao_raio_borda() -> f32 {
    6.0
}

#[inline]
fn padrao_padding() -> f32 {
    8.0
}

/// Configuração do menu de contexto exibido ao clicar com o botão direito.
///
/// Todos os campos são opcionais no TOML e recebem valores padrão quando ausentes.
/// Veja o módulo [`crate::config::context_menu`] para exemplo completo de configuração.
///
/// # Valores padrão
///
/// | Campo | Padrão |
/// |---|---|
/// | `enabled` | `true` |
/// | `background-color` | `[0.12, 0.12, 0.12, 1.0]` (cinza escuro) |
/// | `foreground-color` | `[0.90, 0.90, 0.90, 1.0]` (branco suavizado) |
/// | `selection-color` | `[0.20, 0.47, 0.82, 1.0]` (azul) |
/// | `divider-color` | `[0.30, 0.30, 0.30, 1.0]` (cinza médio) |
/// | `font-size` | `14.0` |
/// | `border-radius` | `6.0` |
/// | `padding` | `8.0` |
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfiguracaoMenuContexto {
    /// Habilita ou desabilita o menu de contexto.
    #[serde(default = "padrao_habilitado", rename = "enabled")]
    pub habilitado: bool,

    /// Cor de fundo do menu (formato hex RGB/RGBA).
    #[serde(
        default = "padrao_cor_fundo",
        deserialize_with = "deserialize_to_arr",
        rename = "background-color"
    )]
    pub cor_fundo: ColorArray,

    /// Cor do texto dos itens do menu (formato hex RGB/RGBA).
    #[serde(
        default = "padrao_cor_texto",
        deserialize_with = "deserialize_to_arr",
        rename = "foreground-color"
    )]
    pub cor_texto: ColorArray,

    /// Cor de fundo do item em destaque (hover/seleção).
    #[serde(
        default = "padrao_cor_hover",
        deserialize_with = "deserialize_to_arr",
        rename = "selection-color"
    )]
    pub cor_hover: ColorArray,

    /// Cor da linha divisória entre grupos de itens.
    #[serde(
        default = "padrao_cor_divisor",
        deserialize_with = "deserialize_to_arr",
        rename = "divider-color"
    )]
    pub cor_divisor: ColorArray,

    /// Tamanho da fonte dos itens em pixels.
    #[serde(default = "padrao_tamanho_fonte", rename = "font-size")]
    pub tamanho_fonte: f32,

    /// Raio das bordas arredondadas do menu em pixels.
    #[serde(default = "padrao_raio_borda", rename = "border-radius")]
    pub raio_borda: f32,

    /// Espaçamento interno (padding) dos itens em pixels.
    #[serde(default = "padrao_padding")]
    pub padding: f32,
}

impl Default for ConfiguracaoMenuContexto {
    fn default() -> Self {
        Self {
            habilitado: padrao_habilitado(),
            cor_fundo: padrao_cor_fundo(),
            cor_texto: padrao_cor_texto(),
            cor_hover: padrao_cor_hover(),
            cor_divisor: padrao_cor_divisor(),
            tamanho_fonte: padrao_tamanho_fonte(),
            raio_borda: padrao_raio_borda(),
            padding: padrao_padding(),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn padrao_tem_menu_habilitado() {
        let config = ConfiguracaoMenuContexto::default();
        assert!(config.habilitado);
    }

    #[test]
    fn padrao_tem_tamanho_fonte_correto() {
        let config = ConfiguracaoMenuContexto::default();
        assert!((config.tamanho_fonte - 14.0).abs() < f32::EPSILON);
    }

    #[test]
    fn padrao_tem_raio_borda_correto() {
        let config = ConfiguracaoMenuContexto::default();
        assert!((config.raio_borda - 6.0).abs() < f32::EPSILON);
    }

    #[test]
    fn padrao_tem_padding_correto() {
        let config = ConfiguracaoMenuContexto::default();
        assert!((config.padding - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn padrao_cor_fundo_tem_alpha_cheio() {
        let config = ConfiguracaoMenuContexto::default();
        assert!((config.cor_fundo[3] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn deserializa_config_minima_com_defaults() {
        let toml = r#"enabled = true"#;
        let config: ConfiguracaoMenuContexto =
            toml::from_str(toml).expect("falha ao desserializar config mínima");
        assert!(config.habilitado);
        assert!((config.tamanho_fonte - 14.0).abs() < f32::EPSILON);
    }

    #[test]
    fn deserializa_config_desabilitada() {
        let toml = r#"enabled = false"#;
        let config: ConfiguracaoMenuContexto =
            toml::from_str(toml).expect("falha ao desserializar config desabilitada");
        assert!(!config.habilitado);
    }

    #[test]
    fn deserializa_tamanho_fonte_customizado() {
        let toml = r#"
            enabled = true
            font-size = 16.0
        "#;
        let config: ConfiguracaoMenuContexto =
            toml::from_str(toml).expect("falha ao desserializar font-size");
        assert!((config.tamanho_fonte - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn deserializa_raio_borda_customizado() {
        let toml = r#"
            enabled = true
            border-radius = 4.0
        "#;
        let config: ConfiguracaoMenuContexto =
            toml::from_str(toml).expect("falha ao desserializar border-radius");
        assert!((config.raio_borda - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn config_vazia_usa_todos_os_defaults() {
        let config: ConfiguracaoMenuContexto =
            toml::from_str("").expect("falha ao desserializar config vazia");
        let padrao = ConfiguracaoMenuContexto::default();
        assert_eq!(config, padrao);
    }

    #[test]
    fn config_e_clone() {
        let config = ConfiguracaoMenuContexto::default();
        let clonado = config.clone();
        assert_eq!(config, clonado);
    }

    #[test]
    fn teste_roundtrip_serialize_deserialize() {
        // O deserializador de cor aceita strings hex — o roundtrip correto parte de TOML hex.
        let toml_entrada = r##"
            enabled = true
            background-color = "#1a1a1aff"
            foreground-color = "#f2f2f2ff"
            selection-color = "#4080d9ff"
            divider-color = "#595959ff"
            font-size = 16.0
            border-radius = 4.0
            padding = 10.0
        "##;
        let config: ConfiguracaoMenuContexto =
            toml::from_str(toml_entrada).expect("falha ao desserializar ConfiguracaoMenuContexto");
        assert!(config.habilitado);
        assert_eq!(config.tamanho_fonte, 16.0);
        assert_eq!(config.raio_borda, 4.0);
        assert_eq!(config.padding, 10.0);
        // Verifica que os arrays de cor foram convertidos (não-zero e com alpha=1.0)
        assert_eq!(config.cor_fundo[3], 1.0);
        assert_eq!(config.cor_texto[3], 1.0);
        assert_eq!(config.cor_hover[3], 1.0);
        assert_eq!(config.cor_divisor[3], 1.0);
    }

    #[test]
    fn teste_parse_toml_vazio_usa_default() {
        let config: ConfiguracaoMenuContexto =
            toml::from_str("").expect("falha ao desserializar toml vazio");
        let padrao = ConfiguracaoMenuContexto::default();
        assert_eq!(config.habilitado, padrao.habilitado);
        assert_eq!(config.tamanho_fonte, padrao.tamanho_fonte);
        assert_eq!(config.raio_borda, padrao.raio_borda);
        assert_eq!(config.padding, padrao.padding);
    }

    /// Verifica que todos os 8 campos do struct default possuem os valores documentados.
    #[test]
    fn teste_default_tem_todos_campos_corretos() {
        let config = ConfiguracaoMenuContexto::default();

        assert!(config.habilitado, "habilitado deve ser true por padrão");

        let cor_fundo_esperada: [f32; 4] = [0.12, 0.12, 0.12, 1.0];
        for (i, (&atual, &esperado)) in config
            .cor_fundo
            .iter()
            .zip(cor_fundo_esperada.iter())
            .enumerate()
        {
            assert!(
                (atual - esperado).abs() < 1e-5,
                "cor_fundo[{i}]: esperado {esperado}, obtido {atual}"
            );
        }

        let cor_texto_esperada: [f32; 4] = [0.90, 0.90, 0.90, 1.0];
        for (i, (&atual, &esperado)) in config
            .cor_texto
            .iter()
            .zip(cor_texto_esperada.iter())
            .enumerate()
        {
            assert!(
                (atual - esperado).abs() < 1e-5,
                "cor_texto[{i}]: esperado {esperado}, obtido {atual}"
            );
        }

        let cor_hover_esperada: [f32; 4] = [0.20, 0.47, 0.82, 1.0];
        for (i, (&atual, &esperado)) in config
            .cor_hover
            .iter()
            .zip(cor_hover_esperada.iter())
            .enumerate()
        {
            assert!(
                (atual - esperado).abs() < 1e-5,
                "cor_hover[{i}]: esperado {esperado}, obtido {atual}"
            );
        }

        let cor_divisor_esperada: [f32; 4] = [0.30, 0.30, 0.30, 1.0];
        for (i, (&atual, &esperado)) in config
            .cor_divisor
            .iter()
            .zip(cor_divisor_esperada.iter())
            .enumerate()
        {
            assert!(
                (atual - esperado).abs() < 1e-5,
                "cor_divisor[{i}]: esperado {esperado}, obtido {atual}"
            );
        }

        assert!(
            (config.tamanho_fonte - 14.0).abs() < f32::EPSILON,
            "tamanho_fonte deve ser 14.0"
        );
        assert!(
            (config.raio_borda - 6.0).abs() < f32::EPSILON,
            "raio_borda deve ser 6.0"
        );
        assert!(
            (config.padding - 8.0).abs() < f32::EPSILON,
            "padding deve ser 8.0"
        );
    }

    #[test]
    fn teste_parse_toml_custom_sobrescreve_defaults() {
        let toml = r#"
            enabled = true
            font-size = 18.0
            border-radius = 10.0
            padding = 12.0
        "#;
        let config: ConfiguracaoMenuContexto =
            toml::from_str(toml).expect("falha ao desserializar config customizada");

        assert!(config.habilitado);
        assert!(
            (config.tamanho_fonte - 18.0).abs() < f32::EPSILON,
            "font-size customizado deve ser 18.0, obtido {}",
            config.tamanho_fonte
        );
        assert!(
            (config.raio_borda - 10.0).abs() < f32::EPSILON,
            "border-radius customizado deve ser 10.0, obtido {}",
            config.raio_borda
        );
        assert!(
            (config.padding - 12.0).abs() < f32::EPSILON,
            "padding customizado deve ser 12.0, obtido {}",
            config.padding
        );

        // Campos não presentes no TOML devem manter os valores padrão
        let padrao = ConfiguracaoMenuContexto::default();
        assert_eq!(
            config.cor_fundo, padrao.cor_fundo,
            "cor_fundo não especificado deve usar padrão"
        );
        assert_eq!(
            config.cor_texto, padrao.cor_texto,
            "cor_texto não especificado deve usar padrão"
        );
        assert_eq!(
            config.cor_hover, padrao.cor_hover,
            "cor_hover não especificado deve usar padrão"
        );
        assert_eq!(
            config.cor_divisor, padrao.cor_divisor,
            "cor_divisor não especificado deve usar padrão"
        );
    }
}
