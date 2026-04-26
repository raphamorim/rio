//! Detecção de ambiente Flatpak e gerenciamento do `rio-pty-host` no host.
//!
//! Este módulo é compilado apenas em Linux via
//! `#[cfg(target_os = "linux")] mod flatpak` em `mod.rs`.
//!
//! # Responsabilidades
//!
//! - Detectar se o processo executa dentro de um sandbox Flatpak via `/.flatpak-info`.
//! - Verificar ou instalar o binário `rio-pty-host` em `~/.local/bin/` no host.
//! - Fornecer o caminho do binário com cache por processo via [`OnceLock`].
//!
//! # Limitações de Teste
//!
//! [`OnceLock`] é global por processo: uma vez inicializado, não pode ser resetado.
//! Testes que precisam verificar estados diferentes (com/sem Flatpak) devem usar
//! subprocessos isolados via `assert_cmd`. Ver módulo `testes` ao final do arquivo.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Cache de detecção Flatpak — inicializado uma única vez por processo.
///
/// Usa `/.flatpak-info` como indicador canônico de ambiente Flatpak,
/// conforme a documentação oficial do projeto Flatpak.
/// Após a primeira chamada a [`detectar_flatpak`], o resultado persiste
/// para todas as invocações subsequentes sem custo adicional de syscall.
static DENTRO_FLATPAK: OnceLock<bool> = OnceLock::new();

/// Retorna `true` se o processo executa dentro de um sandbox Flatpak.
///
/// Verifica a presença de `/.flatpak-info`, que é o indicador canônico
/// criado pelo runtime do Flatpak ao iniciar o sandbox.
///
/// O resultado é calculado uma única vez via [`OnceLock`] e cacheado para
/// todas as invocações subsequentes (custo: 1 syscall `stat` por processo).
/// Esta função é segura para uso em ambientes multi-thread.
pub fn detectar_flatpak() -> bool {
    *DENTRO_FLATPAK.get_or_init(|| std::path::Path::new("/.flatpak-info").exists())
}

/// Cache de disponibilidade do `rio-pty-host` — inicializado uma única vez por processo.
///
/// `None` indica que o binário não está disponível no host; nesse caso o
/// código chamador deve usar o fallback com `flatpak-spawn --host` direto.
/// `Some(path)` contém o caminho absoluto do binário em `~/.local/bin/`.
static RIO_PTY_HOST_DISPONIVEL: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Retorna o caminho do binário `rio-pty-host` no host, se disponível.
///
/// Na primeira chamada, verifica se o binário já existe em
/// `~/.local/bin/rio-pty-host` no host. Se não existir, tenta instalá-lo
/// copiando de `/app/bin/rio-pty-host` (bundled no pacote Flatpak).
///
/// O resultado é cacheado via [`OnceLock`] para todas as chamadas
/// subsequentes sem custo de I/O adicional.
///
/// Retorna `None` sem panic em qualquer caminho de falha (binário não
/// bundled, `flatpak-spawn` indisponível, falha de escrita), ativando
/// o fallback com `flatpak-spawn --host` direto no código chamador.
pub fn caminho_rio_pty_host() -> Option<&'static PathBuf> {
    RIO_PTY_HOST_DISPONIVEL
        .get_or_init(verificar_ou_instalar_rio_pty_host)
        .as_ref()
}

/// Verifica se `rio-pty-host` já existe no host; se não, chama [`instalar_rio_pty_host`].
///
/// Executado uma única vez como inicializador de [`RIO_PTY_HOST_DISPONIVEL`].
/// A verificação usa `flatpak-spawn --host sh -c 'test -x ...'` para consultar
/// o filesystem do host sem sair do sandbox.
fn verificar_ou_instalar_rio_pty_host() -> Option<PathBuf> {
    // Verificar se já existe via flatpak-spawn --host
    let ja_existe = std::process::Command::new("flatpak-spawn")
        .args([
            "--host",
            "sh",
            "-c",
            "test -x ~/.local/bin/rio-pty-host && echo ok",
        ])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "ok")
        .unwrap_or(false);

    if ja_existe {
        let home_host = obter_home_host()?;
        return Some(PathBuf::from(home_host).join(".local/bin/rio-pty-host"));
    }

    instalar_rio_pty_host()
}

/// Instala `rio-pty-host` em `~/.local/bin/` no host copiando de `/app/bin/`.
///
/// Lê o binário de `/app/bin/rio-pty-host` (bundled no sandbox Flatpak) e
/// o escreve no host via `flatpak-spawn --host sh -c '...'`, usando um
/// arquivo temporário + `mv -f` para garantir atomicidade via `rename(2)`.
///
/// A instalação atômica previne corrupção quando múltiplas instâncias do Rio
/// iniciam simultaneamente: todas produzem o mesmo binário sem race condition.
///
/// Retorna `None` em qualquer falha, registrando aviso via `tracing::warn!`.
fn instalar_rio_pty_host() -> Option<PathBuf> {
    let origem = std::path::Path::new("/app/bin/rio-pty-host");

    if !origem.exists() {
        tracing::warn!(
            "rio-pty-host: /app/bin/rio-pty-host não encontrado no sandbox — \
             usando fallback com flatpak-spawn direto"
        );
        return None;
    }

    let conteudo = match std::fs::read(origem) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "rio-pty-host: falha ao ler /app/bin/rio-pty-host: {} — \
                 usando fallback",
                e
            );
            return None;
        }
    };

    // Garantir ~/.local/bin no host (ignorar erro: pode já existir)
    let _ = std::process::Command::new("flatpak-spawn")
        .args(["--host", "sh", "-c", "mkdir -p ~/.local/bin"])
        .status();

    // Instalação atômica: mktemp + cat via stdin + chmod +x + mv -f
    // mv(1) usa rename(2) que é atômico em Linux para arquivos no mesmo filesystem.
    // Múltiplas instâncias simultâneas produzem o mesmo binário — sem corrupção.
    let script_instalacao = r#"
        set -e
        TMP=$(mktemp ~/.local/bin/.rio-pty-host.XXXXXX)
        cat > "$TMP"
        chmod +x "$TMP"
        mv -f "$TMP" ~/.local/bin/rio-pty-host
    "#;

    let mut filho = match std::process::Command::new("flatpak-spawn")
        .args(["--host", "sh", "-c", script_instalacao])
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("rio-pty-host: falha ao iniciar script de instalação: {}", e);
            return None;
        }
    };

    // Escrever conteúdo do binário via stdin do script
    if let Some(mut stdin) = filho.stdin.take() {
        if let Err(e) = stdin.write_all(&conteudo) {
            tracing::warn!("rio-pty-host: falha ao escrever binário via stdin: {}", e);
            let _ = filho.wait();
            return None;
        }
        // stdin é dropado aqui, fechando o pipe e sinalizando EOF ao script
    }

    let status = match filho.wait() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "rio-pty-host: falha ao aguardar conclusão da instalação: {}",
                e
            );
            return None;
        }
    };

    if !status.success() {
        tracing::warn!(
            "rio-pty-host: script de instalação falhou com status {:?}",
            status.code()
        );
        return None;
    }

    let home_host = obter_home_host()?;
    let caminho = PathBuf::from(&home_host).join(".local/bin/rio-pty-host");

    tracing::info!(
        "rio-pty-host: instalado com sucesso em {}",
        caminho.display()
    );

    Some(caminho)
}

/// Obtém o valor de `$HOME` no sistema host via `flatpak-spawn --host`.
///
/// Necessário porque `std::env::var("HOME")` retorna o `$HOME` do sandbox,
/// que pode diferir do `$HOME` do host em algumas configurações Flatpak.
///
/// Retorna `None` se `flatpak-spawn` falhar, não estiver disponível ou
/// produzir saída vazia.
fn obter_home_host() -> Option<String> {
    std::process::Command::new("flatpak-spawn")
        .args(["--host", "sh", "-c", "echo $HOME"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Verifica que `detectar_flatpak` retorna `false` quando `/.flatpak-info` não existe.
    ///
    /// # Por que `#[ignore]`
    ///
    /// `OnceLock` é global por processo: uma vez inicializado por qualquer teste na
    /// mesma execução, não pode ser resetado. Se outro teste invocar `detectar_flatpak()`
    /// antes deste, o resultado já estará cacheado e este teste não pode garantir
    /// o estado inicial. Para validação confiável, use `assert_cmd` para spawnar
    /// um subprocesso isolado que não compartilhe o estado do `OnceLock`.
    #[test]
    #[ignore = "OnceLock global impede reset entre testes; executar como subprocess isolado"]
    fn teste_detectar_flatpak_ausente_em_ambiente_normal() {
        // Em ambiente de CI/desenvolvimento fora de Flatpak, /.flatpak-info não existe
        // e detectar_flatpak() deve retornar false.
        // Se OnceLock já foi inicializado por outro teste com true, este teste pode
        // ser inconlusivo — documentado como limitação de OnceLock.
        let resultado = detectar_flatpak();

        // Em ambiente normal (fora de Flatpak), o resultado esperado é false.
        // Se rodando dentro de Flatpak, o teste é skippado via cfg implícito do ambiente.
        if !std::path::Path::new("/.flatpak-info").exists() {
            assert!(
                !resultado,
                "detectar_flatpak() deve retornar false fora de Flatpak"
            );
        }
    }

    /// Verifica que `instalar_rio_pty_host` retorna `None` quando o binário
    /// de origem não está presente no sandbox (caminho padrão em ambiente de teste).
    ///
    /// Este teste não depende de `OnceLock` global pois chama a função interna
    /// diretamente.
    #[test]
    fn teste_instalar_sem_origem_disponivel_retorna_none() {
        // /app/bin/rio-pty-host não existe em ambiente de desenvolvimento normal
        if std::path::Path::new("/app/bin/rio-pty-host").exists() {
            return; // skipa em ambiente Flatpak real
        }

        let resultado = instalar_rio_pty_host();
        assert!(
            resultado.is_none(),
            "instalar_rio_pty_host() deve retornar None sem /app/bin/rio-pty-host"
        );
    }

    /// Verifica que `obter_home_host` retorna `None` ou `Some` sem panic
    /// (apenas valida que não há `unwrap()` implícito).
    ///
    /// Em ambiente fora de Flatpak, `flatpak-spawn` pode não estar disponível,
    /// então o resultado pode ser `None` — isso é aceitável.
    #[test]
    fn teste_obter_home_host_nao_entra_em_panic() {
        // A função deve retornar Some ou None sem panic em qualquer ambiente
        let _resultado = obter_home_host();
        // Se chegou aqui, não houve panic — teste passa
    }
}
