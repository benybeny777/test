// main.rs - 実行ファイルのエントリ。
//
// Windows では、リリースビルドでコンソール窓を出さない（デスクトップアプリなので、
// 起動のたびに黒い窓が出ると配信画面に映り込む）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    picovtuber_lib::run();
}
