//! strategy-desk.exe entry point. `strategy-desk run --window eu|us` is the
//! headless path the Task Scheduler uses; no arguments opens the window.

fn main() -> anyhow::Result<()> {
    // Stage 1: the egui window lands here (owner: desk-ui agent).
    Ok(())
}
