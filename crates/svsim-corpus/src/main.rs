//! `svsim-corpus` コマンド (`cargo corpus <subcmd>`)。サブコマンドはタスク M0-T12, M0-T13 で実装する。

#![forbid(unsafe_code)]

fn main() {
    eprintln!("svsim-corpus: no subcommands are implemented yet (M0-T12)");
    // 終了コード 2 = 使い方の誤り (plan.md §7.5)
    std::process::exit(2);
}
