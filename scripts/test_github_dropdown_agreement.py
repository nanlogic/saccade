#!/usr/bin/env python3

from __future__ import annotations

import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from probe_github_dropdown_geometry import account_menu_agreement  # noqa: E402


def phase(*, native: bool, shim: bool) -> dict:
    return {
        "after": {
            "route": "profile_button_seen",
            "signOutRect": {"left": 10, "top": 20, "width": 80, "height": 30},
            "signOutHit": native,
            "signOutShimHit": shim,
        }
    }


class GithubDropdownAgreementTest(unittest.TestCase):
    def test_native_green(self):
        report = account_menu_agreement(
            [phase(native=True, shim=False), phase(native=True, shim=False)], "pass"
        )
        self.assertEqual(report["verdict"], "PASS_ACTION_GREEN")
        self.assertEqual(report["recommended_route"], "servo_native")
        self.assertEqual(report["metrics"]["native_hit_test_accuracy"], 1.0)

    def test_verified_shim_does_not_claim_native_green(self):
        report = account_menu_agreement(
            [phase(native=False, shim=True), phase(native=False, shim=True)], "pass"
        )
        self.assertEqual(report["verdict"], "ROUTE_COMPATIBILITY")
        self.assertEqual(
            report["recommended_route"], "servo_with_github_pointer_shim"
        )
        self.assertEqual(report["metrics"]["native_hit_test_accuracy"], 0.0)
        self.assertEqual(report["metrics"]["shim_hit_test_accuracy"], 1.0)
        self.assertIn("AGREEMENT_HIT_TEST_MISMATCH", report["typed_reason_codes"])

    def test_missing_menu_is_incomplete(self):
        report = account_menu_agreement([], "auth_required")
        self.assertEqual(report["verdict"], "INCOMPLETE_EVIDENCE")
        self.assertFalse(report["full_agreement_measured"])


if __name__ == "__main__":
    unittest.main()
