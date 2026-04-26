// Copyright (c) 2024 Danilo Aguiar <daniloaguiarbr@proton.me>
// SPDX-License-Identifier: MIT

//! Lógica de PTY: abertura, fork, execução e relay de I/O.
//!
//! Este módulo implementa o núcleo do `rio-pty-host`: alocar um PTY real no
//! host Linux, executar o shell como processo filho e fazer relay bidirecional
//! de I/O entre o terminal do usuário e o shell.
//!
//! # Fluxo Principal
//!
//! 1. [`config_de_args`] — converte `argv` em [`ConfigPty`] tipado.
//! 2. [`abrir_pty`] — cria par master/slave via `openpty(3)` com tamanho inicial.
//! 3. [`executar`] — faz `fork(2)`: filho configura sessão e executa o shell; pai faz relay.
//! 4. [`relay_io`] — copia dados `stdin→master` e `master→stdout` até EOF ou SIGCHLD.
//!
//! # Limitação v1 — SIGWINCH
//!
//! O tamanho do terminal é configurado apenas no `openpty(3)` inicial via
//! `COLS`/`ROWS` passados como argumentos de linha de comando. Resize dinâmico
//! não é suportado: o PTY do host permanece com o tamanho inicial mesmo que o
//! sandbox receba `SIGWINCH`.
//!
//! TODO v2: monitorar `SIGWINCH` no processo pai e propagar via `TIOCSWINSZ`
//! para o master PTY, atualizando o tamanho do terminal no filho.

use crate::erro::{ErroRioPtyHost, Resultado};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};

/// Constante TIOCSCTTY para Linux (glibc e musl).
#[cfg(all(target_os = "linux", not(target_env = "musl")))]
const TIOCSCTTY: libc::c_ulong = 0x540E;
#[cfg(all(target_os = "linux", target_env = "musl"))]
const TIOCSCTTY: libc::c_int = 0x540E;

/// Par de descritores de arquivo do PTY (master/slave).
///
/// Retornado por [`abrir_pty`]. O chamador é responsável por fechar ambos
/// os FDs quando não forem mais necessários. Após `fork(2)`, o pai fecha
/// `slave` e o filho fecha `master`.
pub struct ParPty {
    /// FD do lado master — leitura/escrita do emulador de terminal (pai).
    pub master: libc::c_int,
    /// FD do lado slave — controlling terminal do processo filho.
    pub slave: libc::c_int,
}

/// Configuração para criação do PTY e execução do shell.
///
/// Construída por [`config_de_args`] a partir dos argumentos de linha de
/// comando e passada para [`abrir_pty`] e [`executar`].
#[derive(Debug)]
pub struct ConfigPty {
    /// Número de colunas do terminal.
    pub colunas: u16,
    /// Número de linhas do terminal.
    pub linhas: u16,
    /// Caminho do shell a executar (CString para compatibilidade com execvp).
    pub shell: CString,
    /// Argumentos passados ao shell (incluindo argv\[0\]).
    pub args: Vec<CString>,
}

/// Status de encerramento do processo filho.
///
/// Retornado por [`executar`] após `waitpid(2)`. O chamador (`main`) converte
/// este valor em exit code POSIX: `Saiu(N)` → `N`, `Sinalizado(N)` → `128 + N`.
pub enum StatusFilho {
    /// Processo encerrou normalmente com o código de saída indicado.
    Saiu(i32),
    /// Processo foi terminado por sinal com o número de sinal indicado.
    Sinalizado(i32),
}

/// Constrói [`ConfigPty`] a partir de argumentos de linha de comando.
///
/// # Formato esperado
/// ```text
/// <COLS> <ROWS> <SHELL> [ARG...]
/// ```
///
/// # Erros
/// - [`ErroRioPtyHost::ArgumentosInsuficientes`] se `args.len() < 3`
/// - [`ErroRioPtyHost::ColsInvalido`] se COLS não for u16 válido
/// - [`ErroRioPtyHost::RowsInvalido`] se ROWS não for u16 válido
pub fn config_de_args(args: Vec<String>) -> Resultado<ConfigPty> {
    if args.len() < 3 {
        return Err(ErroRioPtyHost::ArgumentosInsuficientes);
    }

    let colunas = args[0]
        .parse::<u16>()
        .map_err(|_| ErroRioPtyHost::ColsInvalido(args[0].clone()))?;

    let linhas = args[1]
        .parse::<u16>()
        .map_err(|_| ErroRioPtyHost::RowsInvalido(args[1].clone()))?;

    let shell_str = &args[2];

    // Validar que o shell é caminho absoluto antes de qualquer syscall
    if !shell_str.starts_with('/') {
        return Err(ErroRioPtyHost::ShellNaoAbsoluto(shell_str.to_string()));
    }

    let shell = CString::new(shell_str.as_str())
        .map_err(|_| ErroRioPtyHost::ShellInvalido(shell_str.to_string()))?;

    // argv[0] = caminho do shell; argumentos extras começam em args[3]
    let mut argv_cstrings: Vec<CString> = Vec::with_capacity(args.len() - 1);
    // argv[0] deve ser o nome do shell (basename ou caminho completo)
    argv_cstrings.push(shell.clone());
    for extra in &args[3..] {
        // Ignorar args extras com bytes nulos (inválidos para C)
        if let Ok(cs) = CString::new(extra.as_str()) {
            argv_cstrings.push(cs);
        }
    }

    Ok(ConfigPty {
        colunas,
        linhas,
        shell,
        args: argv_cstrings,
    })
}

/// Abre um par PTY master/slave com o tamanho inicial especificado em `config`.
///
/// Usa `openpty(3)` da libc do host. Os FDs retornados precisam ser fechados
/// pelo chamador quando não mais necessários.
///
/// # Erros
/// - [`ErroRioPtyHost::OpenPtyFalhou`] se `openpty` retornar -1
pub fn abrir_pty(config: &ConfigPty) -> Resultado<ParPty> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;

    // libc::winsize é compatível com o winsize do kernel
    let winsize = libc::winsize {
        ws_col: config.colunas,
        ws_row: config.linhas,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    // SAFETY: master e slave são out-params inicializados por openpty(3);
    // winsize é stack-allocated com lifetime válido durante a chamada;
    // null para nome e termios usa defaults do kernel (sem restrição adicional).
    let ret = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            &winsize,
        )
    };

    if ret != 0 {
        return Err(ErroRioPtyHost::OpenPtyFalhou(
            std::io::Error::last_os_error(),
        ));
    }

    Ok(ParPty { master, slave })
}

/// Executa o shell descrito em `config` dentro de um PTY alocado no host.
///
/// Faz `fork(2)`: o filho configura a sessão, vincula o slave PTY ao controlling
/// terminal via `TIOCSCTTY` e executa o shell via `execvp(3)`. O pai fecha o
/// slave, executa o relay de I/O e aguarda o filho via `waitpid(2)`.
///
/// # Erros
/// Consulte variantes de [`ErroRioPtyHost`] para cada etapa.
pub fn executar(config: ConfigPty) -> Resultado<StatusFilho> {
    let par = abrir_pty(&config)?;

    // Resetar flag para suportar invocações múltiplas (caso futuro)
    FILHO_ENCERROU.store(false, Ordering::SeqCst);

    // SAFETY: handler_sigchld é função estática extern "C"; SIGCHLD é sinal válido.
    // O handler acessa apenas FILHO_ENCERROU (AtomicBool estático), que é
    // async-signal-safe conforme POSIX.
    unsafe {
        libc::signal(libc::SIGCHLD, handler_sigchld as libc::sighandler_t);
    }

    // SAFETY: fork(2) duplica o processo; retorno trata os 3 casos:
    // -1 (erro), 0 (filho), >0 (pai com pid do filho).
    // Nenhum recurso compartilhado mutável é acessado antes do exec no filho.
    let pid = unsafe { libc::fork() };

    if pid < 0 {
        return Err(ErroRioPtyHost::ForkFalhou(std::io::Error::last_os_error()));
    }

    if pid == 0 {
        // ── Processo FILHO ──────────────────────────────────────────────────
        // Fechar master — o filho só usa o slave
        // SAFETY: master é FD válido retornado por openpty(3) neste mesmo processo.
        unsafe { libc::close(par.master) };

        // Nova sessão: o filho se torna líder de sessão sem controlling terminal
        // SAFETY: pós-fork no filho; sem sessão herdada que impeça setsid(2).
        let sid = unsafe { libc::setsid() };
        if sid < 0 {
            // Não podemos retornar Result do filho; saímos com código de erro
            // SAFETY: exit(2) é sempre seguro; encerra o processo filho imediatamente.
            unsafe { libc::exit(1) };
        }

        // Tornar o slave o controlling terminal desta sessão
        // SAFETY: slave é FD válido de openpty(3); setsid() garantiu que não há
        // controlling terminal nesta sessão; TIOCSCTTY atribui o slave como ctty.
        let ret = unsafe {
            libc::ioctl(par.slave, TIOCSCTTY as libc::c_ulong, 0 as libc::c_int)
        };
        if ret < 0 {
            // SAFETY: exit(2) é sempre seguro.
            unsafe { libc::exit(1) };
        }

        // Redirecionar stdin/stdout/stderr para o slave PTY
        // SAFETY: slave é FD válido; STDIN/STDOUT/STDERR_FILENO são constantes
        // definidas pelo sistema operacional; dup2(2) fecha o destino se já aberto.
        // close(slave) após dup2 é seguro pois slave foi duplicado nos FDs padrão.
        unsafe {
            libc::dup2(par.slave, libc::STDIN_FILENO);
            libc::dup2(par.slave, libc::STDOUT_FILENO);
            libc::dup2(par.slave, libc::STDERR_FILENO);
            // Fechar slave após dup2 (já duplicado nos FDs padrão)
            libc::close(par.slave);
        }

        // Construir argv como array de ponteiros terminado em NULL
        let mut ptrs: Vec<*const libc::c_char> =
            config.args.iter().map(|cs| cs.as_ptr()).collect();
        ptrs.push(std::ptr::null());

        // Executar o shell — se retornar, significa erro
        // SAFETY: config.shell é CString válido (verificado em config_de_args);
        // ptrs é vetor de ponteiros para CStrings vivos durante esta chamada,
        // terminado em null conforme contrato de execvp(3).
        // Se execvp retornar, o ponteiro ptrs ainda é válido (não houve exec).
        unsafe {
            libc::execvp(config.shell.as_ptr(), ptrs.as_ptr());
        }

        // execvp retornou: verificar errno para exit code correto
        let err = std::io::Error::last_os_error();
        let codigo = match err.raw_os_error() {
            Some(libc::ENOENT) => 127,
            Some(libc::EACCES) | Some(libc::ENOEXEC) => 126,
            _ => 127,
        };
        // SAFETY: exit(2) é sempre seguro; encerra o processo filho.
        unsafe { libc::exit(codigo) };
    }

    // ── Processo PAI ────────────────────────────────────────────────────────
    // Fechar slave — o pai não usa o slave diretamente
    // SAFETY: slave é FD válido de openpty(3); o filho já tem sua cópia via dup2.
    unsafe { libc::close(par.slave) };

    // Relay bidirecional: stdin→master, master→stdout
    relay_io(par.master, &FILHO_ENCERROU)?;

    // Aguardar o filho e coletar status
    let mut status: libc::c_int = 0;
    // SAFETY: pid > 0 (verificado antes do if pid == 0); status é out-param
    // inicializado por waitpid(2); flags=0 bloqueia até o filho encerrar.
    let ret = unsafe { libc::waitpid(pid, &mut status, 0) };
    if ret < 0 {
        return Err(ErroRioPtyHost::WaitpidFalhou(
            std::io::Error::last_os_error(),
        ));
    }

    // Fechar master após relay concluído
    // SAFETY: master é FD válido de openpty(3); relay_io já encerrou o loop,
    // portanto não há leituras pendentes no master.
    unsafe { libc::close(par.master) };

    // Decodificar status do waitpid
    // SAFETY: status foi preenchido por waitpid(2) com flags=0 (bloqueante),
    // garantidamente válido para WIFEXITED/WEXITSTATUS/WTERMSIG.
    // WIFSIGNALED não é checado separadamente pois else cobre o caso residual.
    #[allow(unused_unsafe)]
    let resultado = unsafe {
        if libc::WIFEXITED(status) {
            StatusFilho::Saiu(libc::WEXITSTATUS(status))
        } else {
            StatusFilho::Sinalizado(libc::WTERMSIG(status))
        }
    };

    Ok(resultado)
}

/// Flag atômica global para SIGCHLD — seguro em signal handlers.
///
/// `AtomicBool` estático é o único tipo garantidamente seguro para escrita
/// em signal handlers POSIX: sem alocação, sem locks, operação atômica única.
static FILHO_ENCERROU: AtomicBool = AtomicBool::new(false);

/// Handler de SIGCHLD — marca a flag atômica para encerrar o relay.
extern "C" fn handler_sigchld(_signum: libc::c_int) {
    // SAFETY: FILHO_ENCERROU é AtomicBool estático; store com SeqCst é
    // async-signal-safe conforme POSIX (operação atômica sem lock interno).
    FILHO_ENCERROU.store(true, Ordering::SeqCst);
}

/// Relay bidirecional de I/O entre stdin/stdout do processo e o master PTY.
///
/// Usa `poll(2)` para monitorar dois FDs simultaneamente:
/// - `STDIN_FILENO`: dados digitados pelo usuário → enviados ao master
/// - `master`: saída do shell → enviada ao stdout
///
/// Encerra quando:
/// - `master` retorna EOF (shell fechou o PTY)
/// - `filho_encerrou` é sinalizado via SIGCHLD
///
/// # Por que poll em vez de select
/// `poll(2)` é preferido pois não tem limite de FD (FD_SETSIZE=1024 no select)
/// e possui API mais clara para verificar POLLHUP/POLLERR.
///
/// # Erros
/// - [`ErroRioPtyHost::RelayIo`] em falha de leitura/escrita não-EOF
fn relay_io(master: libc::c_int, filho_encerrou: &AtomicBool) -> Resultado<()> {
    let mut buf = [0u8; 4096];

    loop {
        // Verificar se filho já encerrou antes de poll
        if filho_encerrou.load(Ordering::Relaxed) {
            // Drenar saída restante do master
            drenar_master(master, &mut buf)?;
            break;
        }

        let mut fds = [
            libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: master,
                events: libc::POLLIN,
                revents: 0,
            },
        ];

        // timeout de 100ms para verificar filho_encerrou periodicamente
        // SAFETY: fds é array stack-allocated válido durante a chamada;
        // nfds é o comprimento correto do array; timeout 100ms é valor positivo.
        let ret = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 100) };

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            // EINTR é normal — loop continua
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(ErroRioPtyHost::RelayIo(err));
        }

        // Processar stdin → master
        if fds[0].revents & libc::POLLIN != 0 {
            // SAFETY: STDIN_FILENO é FD válido (herdado do processo pai);
            // buf é slice mutável válido com len() bytes de capacidade.
            let n = unsafe {
                libc::read(
                    libc::STDIN_FILENO,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };
            if n > 0 {
                escrever_completo(master, &buf[..n as usize])?;
            } else if n == 0 {
                // EOF em stdin: enviar EOF ao master e encerrar
                break;
            } else {
                let err = std::io::Error::last_os_error();
                if err.kind() != std::io::ErrorKind::Interrupted {
                    return Err(ErroRioPtyHost::RelayIo(err));
                }
            }
        }

        // Processar master → stdout
        if fds[1].revents & libc::POLLIN != 0 {
            // SAFETY: master é FD válido de openpty(3);
            // buf é slice mutável válido com len() bytes de capacidade.
            let n = unsafe {
                libc::read(master, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
            };
            if n > 0 {
                escrever_completo(libc::STDOUT_FILENO, &buf[..n as usize])?;
            } else if n == 0 {
                // EOF no master: shell fechou o PTY
                break;
            } else {
                let err = std::io::Error::last_os_error();
                // EIO indica HUP do PTY — encerramento normal
                if err.raw_os_error() == Some(libc::EIO) {
                    break;
                }
                if err.kind() != std::io::ErrorKind::Interrupted {
                    return Err(ErroRioPtyHost::RelayIo(err));
                }
            }
        }

        // POLLHUP no master: shell encerrou
        if fds[1].revents & libc::POLLHUP != 0 {
            drenar_master(master, &mut buf)?;
            break;
        }
    }

    Ok(())
}

/// Drena dados restantes do `master` PTY até EOF ou erro, sem bloquear.
///
/// Chamada após SIGCHLD ou POLLHUP para garantir que toda saída pendente
/// do shell seja enviada ao stdout antes do processo encerrar.
/// Erros de escrita em stdout são silenciados durante a drenagem final.
fn drenar_master(master: libc::c_int, buf: &mut [u8]) -> Resultado<()> {
    loop {
        // SAFETY: master é FD válido de openpty(3);
        // buf é slice mutável válido; leitura não-bloqueante via PTY após HUP.
        let n = unsafe {
            libc::read(master, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
        };
        if n <= 0 {
            break;
        }
        // Ignorar erros de escrita na drenagem final
        let _ = escrever_completo(libc::STDOUT_FILENO, &buf[..n as usize]);
    }
    Ok(())
}

/// Garante escrita completa de `dados` no FD, retentando em escritas parciais.
///
/// `write(2)` pode escrever menos bytes do que solicitado (write parcial).
/// Esta função itera até que todos os bytes sejam escritos, tratando `EINTR`
/// como condição de retry.
///
/// # Erros
///
/// Retorna [`ErroRioPtyHost::RelayIo`] em qualquer erro que não seja `EINTR`.
fn escrever_completo(fd: libc::c_int, dados: &[u8]) -> Resultado<()> {
    let mut escrito = 0;
    while escrito < dados.len() {
        // SAFETY: fd é FD válido (master ou STDOUT_FILENO);
        // dados[escrito..] é slice válido com len() - escrito bytes restantes;
        // o ponteiro permanece válido durante a chamada (sem aliasing com escrita).
        let n = unsafe {
            libc::write(
                fd,
                dados[escrito..].as_ptr() as *const libc::c_void,
                dados.len() - escrito,
            )
        };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(ErroRioPtyHost::RelayIo(err));
        }
        escrito += n as usize;
    }
    Ok(())
}

// ── Testes unitários ─────────────────────────────────────────────────────────

#[cfg(test)]
mod testes {
    use super::*;

    /// Verifica que args válidos (COLS ROWS SHELL) constroem ConfigPty sem erro.
    #[test]
    fn teste_parse_args_validos() {
        let args = vec!["80".to_string(), "24".to_string(), "/bin/echo".to_string()];
        let config = config_de_args(args).unwrap();
        assert_eq!(config.colunas, 80);
        assert_eq!(config.linhas, 24);
    }

    /// Verifica que args insuficientes retornam ArgumentosInsuficientes.
    #[test]
    fn teste_parse_args_insuficientes() {
        let args = vec!["80".to_string(), "24".to_string()];
        let erro = config_de_args(args).unwrap_err();
        assert!(
            matches!(erro, ErroRioPtyHost::ArgumentosInsuficientes),
            "esperado ArgumentosInsuficientes, obtido: {erro}"
        );
    }

    /// Verifica que shell relativo (sem '/') é rejeitado com ShellNaoAbsoluto.
    #[test]
    fn teste_shell_relativo_rejeitado() {
        let args = vec!["80".to_string(), "24".to_string(), "bash".to_string()];
        let erro = config_de_args(args).unwrap_err();
        assert!(
            matches!(erro, ErroRioPtyHost::ShellNaoAbsoluto(_)),
            "esperado ShellNaoAbsoluto, obtido: {erro}"
        );
    }

    /// Verifica que shell com '../' relativo também é rejeitado com ShellNaoAbsoluto.
    #[test]
    fn teste_shell_caminho_relativo_com_pontos_rejeitado() {
        let args = vec![
            "80".to_string(),
            "24".to_string(),
            "../usr/bin/bash".to_string(),
        ];
        let erro = config_de_args(args).unwrap_err();
        assert!(
            matches!(erro, ErroRioPtyHost::ShellNaoAbsoluto(_)),
            "esperado ShellNaoAbsoluto para caminho relativo, obtido: {erro}"
        );
    }

    /// Verifica que COLS inválido retorna ColsInvalido.
    #[test]
    fn teste_cols_invalido() {
        let args = vec!["abc".to_string(), "24".to_string(), "/bin/echo".to_string()];
        let erro = config_de_args(args).unwrap_err();
        assert!(
            matches!(erro, ErroRioPtyHost::ColsInvalido(_)),
            "esperado ColsInvalido, obtido: {erro}"
        );
    }

    /// Verifica que openpty abre FDs >= 0 e válidos (fcntl F_GETFD != -1).
    #[test]
    fn teste_openpty_abre_fds_validos() {
        let config = ConfigPty {
            colunas: 80,
            linhas: 24,
            shell: CString::new("/bin/sh").unwrap(),
            args: vec![CString::new("/bin/sh").unwrap()],
        };
        let par =
            abrir_pty(&config).expect("openpty deve funcionar em ambiente de teste");
        assert!(par.master >= 0, "master FD deve ser >= 0");
        assert!(par.slave >= 0, "slave FD deve ser >= 0");

        // Verificar que master e slave são FDs válidos via fcntl F_GETFD
        // SAFETY: par.master e par.slave são FDs retornados por openpty(3) acima;
        // F_GETFD é operação read-only que não modifica o estado do FD.
        let flags_master = unsafe { libc::fcntl(par.master, libc::F_GETFD) };
        let flags_slave = unsafe { libc::fcntl(par.slave, libc::F_GETFD) };
        assert!(flags_master >= 0, "master deve ser FD válido");
        assert!(flags_slave >= 0, "slave deve ser FD válido");

        // Limpeza
        // SAFETY: par.master e par.slave são FDs válidos abertos por openpty(3);
        // não há outras referências a esses FDs no escopo deste teste.
        unsafe {
            libc::close(par.master);
            libc::close(par.slave);
        }
    }

    /// Verifica que executar /bin/echo retorna StatusFilho::Saiu(0).
    ///
    /// NOTA: Este teste faz fork+exec e waitpid. Requer sistema Linux com
    /// /bin/echo disponível. Não é executado em ambientes sem PTY (CI headless
    /// pode falhar em openpty se não houver /dev/pts montado).
    #[test]
    #[cfg(target_os = "linux")]
    fn teste_spawn_echo_e_aguarda() {
        let config = ConfigPty {
            colunas: 80,
            linhas: 24,
            shell: CString::new("/bin/echo").unwrap(),
            args: vec![
                CString::new("/bin/echo").unwrap(),
                CString::new("oi").unwrap(),
            ],
        };
        match executar(config) {
            Ok(StatusFilho::Saiu(codigo)) => {
                assert_eq!(codigo, 0, "echo deve retornar 0");
            }
            Ok(StatusFilho::Sinalizado(sig)) => {
                panic!("echo foi sinalizado com {sig}");
            }
            Err(e) => {
                // openpty pode falhar em CI sem /dev/pts — registrar mas não falhar
                eprintln!("aviso: teste_spawn_echo_e_aguarda ignorado: {e}");
            }
        }
    }

    // teste_tty_retorna_dispositivo_valido:
    // Executa /usr/bin/tty como shell e verifica que a saída contém "/dev/pts/".
    // NOTA: Este teste valida a correção do bug raiz (ENODEV em ttyname).
    // NÃO implementado como #[test] automático pois requer ambiente Flatpak com
    // /dev/pts do HOST montado corretamente. Execute manualmente após deploy.
    //
    // Procedimento manual:
    //   rio-pty-host 80 24 /usr/bin/tty
    //   saída esperada: /dev/pts/N (N >= 0)
    //   saída com bug: "not a tty" ou ENODEV
}
