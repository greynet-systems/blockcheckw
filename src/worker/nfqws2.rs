use crate::config::CoreConfig;
use crate::error::BlockcheckError;
use crate::system::process::BackgroundProcess;

/// Start nfqws2 as a background process with given queue number and strategy arguments.
pub fn start_nfqws2(
    config: &CoreConfig,
    qnum: u16,
    strategy_args: &[String],
) -> Result<BackgroundProcess, BlockcheckError> {
    let qnum_arg = format!("--qnum={qnum}");
    let fwmark_arg = format!("--fwmark=0x{:08X}", crate::config::DESYNC_MARK);
    let lua_lib = format!(
        "--lua-init=@{}/lua/zapret-lib.lua",
        config.zapret_base
    );
    let lua_antidpi = format!(
        "--lua-init=@{}/lua/zapret-antidpi.lua",
        config.zapret_base
    );

    let mut cmd_owned: Vec<String> = vec![
        config.nfqws2_path.clone(),
        qnum_arg,
        fwmark_arg,
        lua_lib,
        lua_antidpi,
    ];
    cmd_owned.extend_from_slice(strategy_args);

    let cmd_refs: Vec<&str> = cmd_owned.iter().map(|s| s.as_str()).collect();

    BackgroundProcess::spawn(&cmd_refs).map_err(|e| BlockcheckError::Nfqws2Start {
        reason: e.to_string(),
    })
}
