# rio-pty-host


## Propósito
- Binário utilitário que resolve o bug de TTY do Rio Terminal no Flatpak
- Scripts como `gnupg2.sh` chamam `ttyname(3)` para descobrir o dispositivo TTY corrente
- O PTY alocado dentro do sandbox não é visível no host, causando `ENODEV`
- O `rio-pty-host` executa no host, aloca um PTY visível em ambos os lados e faz relay de I/O


## Causa Raiz do Bug
- O Flatpak monta um namespace de `/dev` isolado dentro do sandbox
- `openpty(3)` chamado no sandbox cria `/dev/pts/N` visível SOMENTE dentro do sandbox
- `ttyname(3)` no host falha com `ENODEV` ao tentar resolver esse dispositivo
- Scripts que dependem de `ttyname` (gpg-agent, pass, gnupg2) tornam-se inutilizáveis


## Como Funciona
- O Rio detecta o ambiente Flatpak via `/.flatpak-info` ao iniciar
- O módulo `teletypewriter::unix::flatpak` instala `rio-pty-host` em `~/.local/bin/` no host
- Cada nova janela de terminal invoca `flatpak-spawn --host ~/.local/bin/rio-pty-host COLS ROWS SHELL`
- O `rio-pty-host` chama `openpty(3)` no host, obtendo um PTY real do host
- O processo faz `fork(2)`: o filho configura a sessão e executa o shell; o pai faz relay
- O relay usa `poll(2)` para copiar `stdin→master` e `master→stdout` de forma bidirecional
- O SIGCHLD do filho encerra o relay e o pai coleta o exit code via `waitpid(2)`


## Build
- Compilar apenas o binário: `cargo build --release -p rio-pty-host`
- O binário resultante fica em `target/release/rio-pty-host`
- Tamanho esperado: aproximadamente 300-500 KB (dependente de linking estático/dinâmico)
- Para linking estático com musl: `cargo build --release -p rio-pty-host --target x86_64-unknown-linux-musl`


## Uso Manual
- Protocolo de argumentos: `rio-pty-host <COLS> <ROWS> <SHELL> [ARG...]`
- Exemplo com bash: `rio-pty-host 80 24 /bin/bash`
- Exemplo com zsh e argumento: `rio-pty-host 120 40 /usr/bin/zsh -i`
- Exit codes POSIX: `0` sucesso, `N` (1-125) código do shell, `126` EACCES/ENOEXEC, `127` ENOENT, `1` erro interno


## Distribuição via Flatpak
- O binário é bundled em `/app/bin/rio-pty-host` dentro do pacote Flatpak
- Na primeira inicialização do Rio, o módulo `teletypewriter::unix::flatpak` o instala
- A instalação copia para `~/.local/bin/rio-pty-host` no host via `flatpak-spawn`
- A instalação é atômica: usa `mktemp` + `chmod +x` + `mv -f` (`rename(2)`) para evitar race conditions
- Múltiplas instâncias do Rio iniciando simultaneamente não causam corrupção do binário
- O caminho resultante é cacheado via `OnceLock` para invocações subsequentes sem I/O adicional


## Limitações
- v1: `SIGWINCH` não é propagado do sandbox para o processo filho no host
- O tamanho do terminal é fixado nos `COLS`/`ROWS` passados na linha de comando
- Resize dinâmico da janela não atualiza o PTY do host enquanto o shell executa
- v2 planejado: monitorar `SIGWINCH` no pai e propagar via `TIOCSWINSZ` ao master PTY


## Licença
- MIT — mesma licença do projeto Rio Terminal
- Arquivo `LICENSE` na raiz do repositório
