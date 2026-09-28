fn main() -> anyhow::Result<()> {
    fpt_metal::run_cli(&std::env::args().skip(1).collect::<Vec<_>>())
}
