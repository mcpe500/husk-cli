use husk_cli::config::{Config, ProviderPreset};

#[test]
fn test_config_provider_presets() {
    let mut cfg = Config::default();

    // Test GLM Z.AI Anthropic preset (glm-4 was retired by Z.AI; verified via docs.z.ai pricing)
    cfg.apply_preset(ProviderPreset::ZaiAnthropic);
    assert_eq!(cfg.base_url, "https://api.z.ai/api/anthropic");
    assert_eq!(cfg.model, "glm-5.2");

    // Test GLM Z.AI OpenAI preset
    cfg.apply_preset(ProviderPreset::ZaiOpenAi);
    assert_eq!(cfg.base_url, "https://api.z.ai/api/coding/paas/v4");
    assert_eq!(cfg.model, "glm-5.2");

    // Test MiniMax OpenAI preset
    cfg.apply_preset(ProviderPreset::MiniMaxOpenAi);
    assert_eq!(cfg.base_url, "https://api.minimax.io/v1");
    assert_eq!(cfg.model, "MiniMax-M3");

    // Test MiniMax Anthropic preset
    cfg.apply_preset(ProviderPreset::MiniMaxAnthropic);
    assert_eq!(cfg.base_url, "https://api.minimax.io/anthropic");
    assert_eq!(cfg.model, "MiniMax-M3");

    // Test DeepSeek V4 Flash 0731 preset via Netra Runtime (user's provider)
    cfg.apply_preset(ProviderPreset::Deepseek);
    assert_eq!(cfg.base_url, "https://api.netraruntime.com/v1");
    assert_eq!(cfg.model, "deepseek/deepseek-v4-flash-0731");
}
