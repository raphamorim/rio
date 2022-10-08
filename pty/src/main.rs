mod pty;

use std::env;
use pty::{fork, open};
use filedescriptor::{FileDescriptor, FromRawFileDescriptor, Result};
use std::io::Write;

fn main() {
	env::set_var("TERM", "rio");

    let f = match fork(
        "sh".to_string(),
        vec![],
        vec![
"TERM_PROGRAM=Apple_Terminal".to_string(),
"SHELL=/bin/zsh".to_string(),
"TERM=rio".to_string(),
"TMPDIR=/var/folders/g9/95ghd8tn5gs5nx4y4zm25p940000gn/T/".to_string(),
"TERM_PROGRAM_VERSION=445".to_string(),
"TERM_SESSION_ID=20D593E2-D4F3-4FB6-8988-552EDC85001B".to_string(),
"USER=hugoamor".to_string(),
"SSH_AUTH_SOCK=/private/tmp/com.apple.launchd.rimCPg5XiB/Listeners".to_string(),
"PATH=/usr/local/opt/qt/bin:/Users/hugoamor/.nvm/versions/node/v12.22.0/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/opt/X11/bin:/Users/hugoamor/.cargo/bin".to_string(),
"__CFBundleIdentifier=com.apple.Terminal".to_string(),
"PWD=/Users/hugoamor/Documents/personal/rio/pty".to_string(),
"XPC_FLAGS=0x0".to_string(),
"XPC_SERVICE_NAME=0".to_string(),
"SHLVL=1".to_string(),
"HOME=/Users/hugoamor".to_string(),
"LOGNAME=hugoamor".to_string(),
"DISPLAY=/private/tmp/com.apple.launchd.NP2s8T407F/org.xquartz:0".to_string(),
"OLDPWD=/Users/hugoamor/Documents/personal/rio".to_string(),
"PROMPT_EOL_MARK=".to_string(),
"CONDA_CHANGEPS1=no".to_string(),
"NVM_DIR=/Users/hugoamor/.nvm".to_string(),
"NVM_CD_FLAGS=-q".to_string(),
"NVM_BIN=/Users/hugoamor/.nvm/versions/node/v12.22.0/bin".to_string(),
"NVM_INC=/Users/hugoamor/.nvm/versions/node/v12.22.0/include/node".to_string(),
"VIRTUAL_ENV_DISABLE_PROMPT=12".to_string(),
"LC_CTYPE=UTF-8".to_string(),
"_=/usr/bin/env".to_string()],
        "/Users/hugoamor".to_string(),
        80,
        24,
        -1,
        -1,
        true,
    ) {
        Ok(f) => {
            println!("{:?}", f);

            let a = open(80, 24);
            println!("{:?}", a);

            f
        }
        Err(_) => todo!(),
    };

    println!("{:?}", f);
    println!("{:?}", get_stdout());
    println!("{:?}", print_something());

}

fn get_stdout() -> Result<FileDescriptor> {
  let stdout = std::io::stdout();
  let handle = stdout.lock();
  FileDescriptor::dup(&handle)
}

fn print_something() -> Result<()> {
   get_stdout()?.write(b"hello")?;
   Ok(())
}