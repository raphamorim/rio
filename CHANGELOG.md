# Changelog

Todas as mudanças notáveis neste fork do Rio terminal são documentadas aqui.

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/)
e este projeto adota [Versionamento Semântico](https://semver.org/lang/pt-BR/).


## [Não lançado]

### Adicionado
- Menu de contexto exibido ao clicar com botão direito no terminal
  (`[context-menu]` no `config.toml`) com ações: Copiar, Colar, Selecionar Tudo,
  Dividir Horizontalmente, Dividir Verticalmente, Nova Aba, Fechar Aba
- Configuração visual completa do menu de contexto: cor de fundo, cor de texto,
  cor de seleção, cor de divisor, tamanho de fonte, raio de borda e padding
  (`rio-backend/src/config/context_menu.rs`)
- Destaque visual da divisão ativa: borda colorida ao redor do painel com foco
  quando há múltiplas divisões (`navigation.highlight-active-split`,
  `navigation.active-split-color`)
- Override por plataforma das novas opções de navegação via bloco
  `[platform.navigation]` no `config.toml`

### Modificado
- `rio-backend/src/config/navigation.rs`: campos `destacar_pane_ativo` e
  `cor_borda_pane_ativo` adicionados à struct `Navigation`
- `rio-backend/src/config/platform.rs`: campos opcionais correspondentes
  adicionados à struct `PlatformNavigation` para override por plataforma
- `rio-backend/src/config/mod.rs`: módulo `context_menu` publicado; campo
  `menu_contexto` adicionado à struct `Config`; lógica de merge de plataforma
  para os novos campos de navegação
- `frontends/rioterm/src/renderer/mod.rs`: campos `destacar_pane_ativo`,
  `cor_borda_pane_ativo`, `context_menu` e `config_menu_contexto` adicionados
  à struct `Renderer`; lógica de renderização do destaque de painel ativo e
  delegação de renderização ao `MenuContexto`

### Correções
- Warnings de compilação `function_casts_as_integer` removidos em
  `rio-pty-host` (compatibilidade com Rust 2024)


## Sobre este fork

Este repositório é um fork de [raphamorim/rio](https://github.com/raphamorim/rio)
mantido por [@daniloaguiarbr](https://github.com/daniloaguiarbr).

O objetivo principal é adicionar suporte a ambientes Flatpak detectando
automaticamente a execução em sandbox e delegando operações de PTY ao
host via `flatpak-spawn --host`.
