use std::path::PathBuf;

use eyre::WrapErr as _;

use crate::genesis_args::GenesisArgs;

#[derive(clap::Parser, Debug)]
pub(crate) struct GenerateGenesis {
    /// Output file path
    #[arg(short, long)]
    output: PathBuf,

    #[clap(flatten)]
    genesis_args: GenesisArgs,
}

impl GenerateGenesis {
    pub(crate) async fn run(self) -> eyre::Result<()> {
        let (genesis, validators) = self
            .genesis_args
            .generate_genesis_and_consensus_config()
            .await
            .wrap_err("failed generating genesis")?;

        for (pub_key, validator) in validators.validators {
            let ed25519_dst = self.output.with_file_name(format!("{pub_key}.signing"));
            std::fs::write(&ed25519_dst, validator.encode_ed25519_private_key()).wrap_err_with(
                || {
                    format!(
                        "failed to write ed25519 private key to file `{}`",
                        ed25519_dst.display()
                    )
                },
            )?;
            let bls12381_dst = self.output.with_file_name(format!("{pub_key}.share"));
            std::fs::write(&ed25519_dst, validator.encode_bls12381_private_key_share())
                .wrap_err_with(|| {
                    format!(
                        "failed to write bls12381 private key shaer to file `{}`",
                        bls12381_dst.display()
                    )
                })?;
        }
        let json =
            serde_json::to_string_pretty(&genesis).wrap_err("failed encoding genesis as JSON")?;
        std::fs::write(&self.output, json).wrap_err_with(|| {
            format!(
                "failed writing genesiss to file `{}`",
                self.output.display()
            )
        })?;
        Ok(())
    }
}
