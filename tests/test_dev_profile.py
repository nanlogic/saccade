import json
import tempfile
import unittest
from pathlib import Path

from scripts.dev_profile import install, resolve_profile_source, validate


class DevelopmentProfileTests(unittest.TestCase):
    def test_default_profile_requires_autonomous_safe_completion(self) -> None:
        profile = json.loads((Path(__file__).parents[1] / "profiles" / "default.json").read_text())
        self.assertIn("no suitable Agent-owned tab exists", profile["behavior"])
        self.assertIn("never ask permission first", profile["behavior"])
        self.assertIn("exactly one closed yes/no question", profile["behavior"])
        self.assertIn("faster than the user", profile["behavior"])
        self.assertIn("MCP adds no safety taxonomy or action gate", profile["behavior"])
        self.assertIn("Agent-Off tabs remain unreadable", profile["behavior"])
        self.assertIn("close Agent-owned tabs opened only for temporary research", profile["behavior"])
        self.assertIn("every user-shared tab", profile["behavior"])
        self.assertIn("deferred or lazy registry", profile["behavior"])
        self.assertIn("instead of silently falling back", profile["behavior"])

    def test_ceo_profile_distinguishes_ordinary_profile_data_from_secrets(self) -> None:
        profile = json.loads(
            (Path(__file__).parents[1] / "profiles" / "smart-barbarian-ceo.json").read_text()
        )
        self.assertEqual(validate(profile), profile)
        self.assertEqual(profile["ban"], [])
        self.assertIn("自动 Agent On", profile["behavior"])
        self.assertIn("Agent-Off 标签页保持不可读", profile["behavior"])
        self.assertIn("不增加安全分类或动作闸门", profile["behavior"])
        self.assertIn("全自动首席执行官", profile["behavior"])
        self.assertIn("直接执行最终动作并验证结果", profile["behavior"])
        self.assertIn("执行这个推荐方案吗？yes/no", profile["behavior"])
        self.assertIn("绝不读取、复述、记录、猜测", profile["behavior"])
        self.assertIn("立即自动恢复、完成提交并验证", profile["behavior"])
        self.assertIn("完整稳定 ID catalog", profile["behavior"])
        self.assertIn("只按与任务相关的 object_id 取一次 details", profile["behavior"])
        self.assertIn("禁止重复初始读取", profile["behavior"])

    def test_install_preserves_three_fields_and_unicode(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source.json"
            destination = root / "runtime" / "profile.json"
            expected = {"name": "聪明的野蛮人 CEO", "behavior": "直接推进。", "ban": []}
            source.write_text(json.dumps(expected, ensure_ascii=False), encoding="utf-8")
            self.assertEqual(install(source, destination), expected)
            self.assertEqual(json.loads(destination.read_text(encoding="utf-8")), expected)
            self.assertEqual(destination.stat().st_mode & 0o777, 0o600)

    def test_unknown_fields_and_empty_conditions_fail(self) -> None:
        with self.assertRaises(ValueError):
            validate({"name": "bad", "behavior": "", "ban": [], "extra": True})
        with self.assertRaises(ValueError):
            validate({"name": "bad", "behavior": "", "ban": [{"control": "Save", "condition": ""}]})

    def test_legacy_eco_profile_name_migrates_to_ceo(self) -> None:
        profiles = Path(__file__).parents[1] / "profiles"
        source = resolve_profile_source("smart-barbarian-eco", profiles)
        self.assertEqual(source, profiles / "smart-barbarian-ceo.json")
        self.assertEqual(validate(json.loads(source.read_text()))["name"], "聪明的野蛮人 CEO")


if __name__ == "__main__":
    unittest.main()
