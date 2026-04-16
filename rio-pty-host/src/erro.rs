// Copyright (c) 2024 Danilo Aguiar <daniloaguiarbr@proton.me>
// SPDX-License-Identifier: MIT

//! Tipos de erro do `rio-pty-host`.
//!
//! Define [`ErroRioPtyHost`] com todas as condições de falha possíveis durante
//! a alocação de PTY, o fork e o relay de I/O, e o alias [`Resultado`] para
//! uso conveniente em toda a codebase do crate.

use thiserror::Error;

/// Erros possíveis durante a execução do `rio-pty-host`.
///
/// Cobre todas as etapas do ciclo de vida: parse de argumentos, alocação
/// de PTY, fork, configuração de sessão no filho e relay de I/O no pai.
///
/// Algumas variantes (`SetsidFalhou`, `TiocscttyFalhou`, `ExecvpFalhou`) são
/// geradas no processo filho após `fork(2)`, que não pode retornar `Result`
/// ao pai. Estão definidas aqui para documentar o contrato e permitir
/// extensão futura (por exemplo, via pipe de erros pai-filho).
///
/// Todas as variantes implementam [`std::error::Error`] via `#[derive(thiserror::Error)]`
/// e preservam a causa original com `#[source]` onde aplicável.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum ErroRioPtyHost {
    /// Argumentos insuficientes na linha de comando.
    #[error("argumentos insuficientes: esperado COLS ROWS SHELL [ARG...]")]
    ArgumentosInsuficientes,

    /// Valor de COLS inválido (não é u16).
    #[error("COLS inválido: {0}")]
    ColsInvalido(String),

    /// Valor de ROWS inválido (não é u16).
    #[error("ROWS inválido: {0}")]
    RowsInvalido(String),

    /// Chamada openpty falhou.
    #[error("openpty falhou: {0}")]
    OpenPtyFalhou(#[source] std::io::Error),

    /// Chamada fork falhou.
    #[error("fork falhou: {0}")]
    ForkFalhou(#[source] std::io::Error),

    /// Chamada setsid falhou.
    #[error("setsid falhou: {0}")]
    SetsidFalhou(#[source] std::io::Error),

    /// Ioctl TIOCSCTTY falhou.
    #[error("TIOCSCTTY falhou: {0}")]
    TiocscttyFalhou(#[source] std::io::Error),

    /// Chamada execvp falhou.
    #[error("execvp falhou: {0}")]
    ExecvpFalhou(#[source] std::io::Error),

    /// Erro de I/O no relay PTY.
    #[error("I/O no relay PTY: {0}")]
    RelayIo(#[source] std::io::Error),

    /// Chamada waitpid falhou.
    #[error("waitpid falhou: {0}")]
    WaitpidFalhou(#[source] std::io::Error),

    /// Shell não é caminho absoluto (deve começar com '/').
    #[error("shell deve ser caminho absoluto: {0}")]
    ShellNaoAbsoluto(String),

    /// Shell contém bytes nulos ou é inválido como CString.
    #[error("shell inválido: {0}")]
    ShellInvalido(String),
}

/// Alias de `Result` parametrizado com [`ErroRioPtyHost`].
///
/// Simplifica assinaturas de função em todo o crate, eliminando a necessidade
/// de repetir o tipo de erro explicitamente em cada `Result<T, ErroRioPtyHost>`.
pub type Resultado<T> = Result<T, ErroRioPtyHost>;
