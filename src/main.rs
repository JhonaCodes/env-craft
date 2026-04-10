use envcraft::{Cli, run};

fn main() -> anyhow::Result<()> {
    let cli = <Cli as clap::Parser>::parse();
    run(cli)
}
