// cloud_free.rs - クラウド推論の混入を検出する設計ガード（テスト専用）。
//
// このアプリの約束は「生成も認識も判定も、利用者のPCの中だけで行う」こと。約束は
// README にも書いてあるが、文章は破っても落ちない。**破ったらビルドが落ちる**形に
// しておかないと、「精度が上がるから」「無料枠があるから」で少しずつ入り込む。
//
// ネットワーク自体は禁じない。禁じるのは**推論を外部サービスへ投げること**で、
// モデル重み・ランタイム・アプリ更新の取得は許す。だから判定はホスト名で行う。

/// クラウド推論サービスのホスト。ここにある文字列がソースへ入ったら落とす。
///
/// 増やすときは「そのホストが推論を受け付けるか」で判断する。配布物の取得元
/// （GitHub Releases、Hugging Face のファイル配信など）はここへ入れない。
const INFERENCE_HOSTS: [&str; 10] = [
    "api.openai.com",
    "api.anthropic.com",
    "generativelanguage.googleapis.com",
    "api.mistral.ai",
    "api.deepseek.com",
    "api.x.ai",
    "api.cohere.ai",
    "api.stability.ai",
    "api.elevenlabs.io",
    "api.tripo3d.ai",
];

/// 秘密鍵らしき環境変数名。クラウド推論の入口はたいていここから始まる。
const INFERENCE_KEY_NAMES: [&str; 4] = [
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GOOGLE_API_KEY",
    "STABILITY_API_KEY",
];

#[cfg(test)]
mod guard {
    use super::{INFERENCE_HOSTS, INFERENCE_KEY_NAMES};
    use crate::guard_scan;

    #[test]
    fn クラウド推論サービスへ繋がない() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "cloud_free.rs" {
                continue; // 判定の定義元（このリスト自身）
            }
            let body = source.body();
            for host in INFERENCE_HOSTS {
                if body.contains(host) {
                    offenders.push(format!("{}: {host}", source.path.display()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "クラウド推論サービスへの接続を追加しないでください\
             （このアプリの約束は、生成も認識も判定も利用者のPC内で完結することです）: {offenders:?}"
        );
    }

    #[test]
    fn クラウド推論の資格情報を持たない() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "cloud_free.rs" {
                continue; // 定義元
            }
            let body = source.body();
            for name in INFERENCE_KEY_NAMES {
                if body.contains(name) {
                    offenders.push(format!("{}: {name}", source.path.display()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "クラウド推論の資格情報を扱わないでください（鍵があるということは送る先があるということです）: {offenders:?}"
        );
    }

    /// 検出器が実際に効くこと。効かないガードは無いのと同じ。
    #[test]
    fn 検出器がホスト名と鍵名を見落とさない() {
        assert!(INFERENCE_HOSTS.contains(&"api.openai.com"));
        assert!(INFERENCE_KEY_NAMES.contains(&"OPENAI_API_KEY"));
        // 配布物の取得元は禁止対象に入れない（重みの取得までできなくなる）。
        assert!(!INFERENCE_HOSTS.iter().any(|host| host.contains("github")));
        assert!(!INFERENCE_HOSTS
            .iter()
            .any(|host| host.contains("huggingface")));
    }
}
