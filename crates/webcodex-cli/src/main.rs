#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match codexlink_product_boundary::admit_current_process(std::env::args_os().skip(1)) {
        Ok(codexlink_product_boundary::Admission::Operational) => {
            if let Err(error) = codexlink_product_boundary::require_cli_runtime() {
                eprintln!("{error}");
                std::process::exit(78);
            }
        }
        Ok(codexlink_product_boundary::Admission::Diagnostic) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(78);
        }
    }
    webcodex_cli::run().await
}
