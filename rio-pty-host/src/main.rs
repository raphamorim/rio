// Copyright (c) 2024 Danilo Aguiar <daniloaguiarbr@proton.me>
// SPDX-License-Identifier: MIT

//! Binário utilitário que aloca PTY no host quando Rio executa em Flatpak.
//!
//! # Problema
//!
//! Quando o Rio Terminal executa dentro de um sandbox Flatpak, o processo
//! recebe um PTY alocado pelo runtime do sandbox. Scripts como `gnupg2.sh`
//! chamam `ttyname(3)` para descobrir o dispositivo TTY corrente — mas o
//! dispositivo `/dev/pts/N` do sandbox não é visível no host, causando
//! `ENODEV` e tornando o terminal inutilizável para esses scripts.
//!
//! # Solução
//!
//! O `rio-pty-host` executa **no host** (fora do sandbox) via
//! `flatpak-spawn --host`. Ele chama `openpty(3)` no host, obtendo um PTY
//! visível tanto no host quanto no sandbox, e faz relay bidirecional de I/O
//! entre o terminal do usuário e o shell.
//!
//! # Uso
//!
//! ```text
//! rio-pty-host <COLS> <ROWS> <SHELL> [ARG...]
//! ```
//!
//! - `COLS` — largura inicial do terminal (u16)
//! - `ROWS` — altura inicial do terminal (u16)
//! - `SHELL` — caminho absoluto do shell a executar
//! - `ARG...` — argumentos opcionais passados ao shell
//!
//! # Exit Codes
//!
//! | Código | Significado                                      |
//! |--------|--------------------------------------------------|
//! | `0`    | shell encerrou com sucesso                       |
//! | `N`    | código de saída do shell (1–125)                 |
//! | `126`  | erro ao executar o shell (`EACCES` ou `ENOEXEC`) |
//! | `127`  | shell não encontrado (`ENOENT`)                  |
//! | `1`    | erro interno (parse de args, openpty, fork)      |
//!
//! # Integração com o Rio
//!
//! O módulo `teletypewriter::unix::flatpak` detecta o ambiente Flatpak,
//! instala o binário em `~/.local/bin/rio-pty-host` e o invoca via
//! `flatpak-spawn --host` ao criar cada nova janela de terminal.

mod erro;
mod pty;

use pty::StatusFilho;
use std::process;

fn main() {
    // Coletar args ignorando argv[0] (nome do binário)
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Construir configuração a partir dos argumentos
    let config = match pty::config_de_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("rio-pty-host: {e}");
            process::exit(1);
        }
    };

    // Executar shell dentro do PTY alocado no host
    match pty::executar(config) {
        Ok(StatusFilho::Saiu(codigo)) => {
            process::exit(codigo);
        }
        Ok(StatusFilho::Sinalizado(sinal)) => {
            // Convenção POSIX: exit 128+N para processo terminado por sinal N
            process::exit(128 + sinal);
        }
        Err(e) => {
            eprintln!("rio-pty-host: {e}");
            // Mapear erro de execvp para exit codes padronizados
            let codigo = match &e {
                erro::ErroRioPtyHost::ExecvpFalhou(io_err) => {
                    match io_err.raw_os_error() {
                        Some(libc::ENOENT) => 127,
                        Some(libc::EACCES) | Some(libc::ENOEXEC) => 126,
                        _ => 1,
                    }
                }
                _ => 1,
            };
            process::exit(codigo);
        }
    }
}
