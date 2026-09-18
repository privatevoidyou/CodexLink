#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match codexlink_product_boundary::admit_current_process(std::env::args_os().skip(1)) {
        Ok(codexlink_product_boundary::Admission::Continue) => {}
        Ok(codexlink_product_boundary::Admission::ExitSuccess(message)) => {
            println!("{message}");
            return Ok(());
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(78);
        }
    }
    webcodex_cli::run().await
}
