use vergen_gitcl::{BuildBuilder, CargoBuilder, Emitter, GitclBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let build = BuildBuilder::default().build_date(true).build()?;
    let cargo = CargoBuilder::default().target_triple(true).build()?;
    let gitcl = GitclBuilder::default().sha(true).build()?;

    let result = Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&cargo)?
        .add_instructions(&gitcl)?
        .emit();

    // git コマンドが失敗した場合、GITHUB_SHA 環境変数にフォールバック
    if result.is_err() {
        println!("cargo::rustc-env=VERGEN_BUILD_DATE=unknown");
        println!("cargo::rustc-env=VERGEN_CARGO_TARGET_TRIPLE=unknown");
        if let Ok(sha) = std::env::var("GITHUB_SHA") {
            println!(
                "cargo::rustc-env=VERGEN_GIT_SHA={}",
                &sha[..7.min(sha.len())]
            );
        } else {
            println!("cargo::rustc-env=VERGEN_GIT_SHA=unknown");
        }
    }

    Ok(())
}
