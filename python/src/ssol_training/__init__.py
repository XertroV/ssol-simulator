"""SSOL Training Infrastructure for A Slower Speed of Light."""

__all__ = [
    "SSOLEnv",
    "make_ssol_env",
    "SSOLFeatureExtractor",
    "SSOLFeatureExtractorLight",
]


def __getattr__(name: str):
    if name in ("SSOLEnv", "make_ssol_env"):
        from .ssol_env import SSOLEnv, make_ssol_env

        return {"SSOLEnv": SSOLEnv, "make_ssol_env": make_ssol_env}[name]
    if name in ("SSOLFeatureExtractor", "SSOLFeatureExtractorLight"):
        from .feature_extractor import SSOLFeatureExtractor, SSOLFeatureExtractorLight

        return {
            "SSOLFeatureExtractor": SSOLFeatureExtractor,
            "SSOLFeatureExtractorLight": SSOLFeatureExtractorLight,
        }[name]
    raise AttributeError(name)
