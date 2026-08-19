import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "run_claude_same_tab", ROOT / "scripts/run_claude_same_tab.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def build_command(tab_id: str = "4242", url: str = "http://127.0.0.1/test") -> list[str]:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        return MODULE.command(root / "claude", root / "runtime", root / "state", url, tab_id)


def assistant(*blocks: dict) -> dict:
    return {"type": "assistant", "message": {"content": list(blocks)}}


def user(*blocks: dict) -> dict:
    return {"type": "user", "message": {"content": list(blocks)}}


class ClaudeSameTabTests(unittest.TestCase):
    def test_command_is_strict_saccade_plus_client_chrome_only(self) -> None:
        command = build_command()
        joined = " ".join(command)
        self.assertIn("--chrome", command)
        self.assertIn("--strict-mcp-config", command)
        self.assertIn("Bash,WebFetch,WebSearch", command)
        self.assertIn('"saccade"', joined)
        config = command[command.index("--mcp-config") + 1]
        self.assertNotIn("playwright", config.casefold())

    def test_prompt_names_the_preopened_tab_and_forbids_a_duplicate(self) -> None:
        prompt = build_command(tab_id="991155")[2]
        self.assertIn("991155", prompt)
        self.assertIn("ALREADY opened", prompt)
        self.assertIn("tabs_context_mcp", prompt)
        # A second tab on the same URL is the failure this probe exists to catch.
        self.assertIn("do not open a second copy", prompt.casefold())
        self.assertNotIn("saccade.tabs.open", prompt.split("Do not call")[0])

    def test_execution_tab_ids_come_from_chrome_calls_only(self) -> None:
        events = [
            assistant({"type": "tool_use", "name": "mcp__saccade__saccade_truth_read",
                       "input": {"tab_id": "7", "tabId": 999}}),
            assistant({"type": "tool_use", "name": "mcp__claude-in-chrome__computer",
                       "input": {"action": "left_click", "tabId": 7}}),
        ]
        self.assertEqual(MODULE.chrome_execution_tab_ids(events), ["7"])

    def test_tab_management_calls_are_not_execution(self) -> None:
        # Closing its own scratch tab must not be read as same-tab execution.
        events = [
            assistant({"type": "tool_use", "name": "mcp__claude-in-chrome__tabs_close_mcp",
                       "input": {"tabId": 55}}),
            assistant({"type": "tool_use", "name": "mcp__claude-in-chrome__tabs_create_mcp",
                       "input": {"tabId": 56}}),
        ]
        self.assertEqual(MODULE.chrome_execution_tab_ids(events), [])

    def test_prompt_does_not_reshuffle_tab_groups(self) -> None:
        prompt = build_command()[2]
        self.assertIn("Do not close, create, or reshuffle Chrome tab groups", prompt)

    def test_chrome_tool_failures_are_kept_for_diagnosis(self) -> None:
        events = [
            assistant({"type": "tool_use", "id": "a",
                       "name": "mcp__claude-in-chrome__computer", "input": {"tabId": 7}}),
            user({"type": "tool_result", "tool_use_id": "a", "is_error": True,
                  "content": "Couldn't determine which page this action targets."}),
        ]
        failures = MODULE.chrome_tool_failures(events)
        self.assertEqual(len(failures), 1)
        self.assertIn("Couldn't determine which page", failures[0])

    def test_saccade_errors_are_not_attributed_to_chrome(self) -> None:
        events = [
            assistant({"type": "tool_use", "id": "s",
                       "name": "mcp__saccade__saccade_truth_read", "input": {"tab_id": "7"}}),
            user({"type": "tool_result", "tool_use_id": "s", "is_error": True,
                  "content": "region view requires document_id"}),
        ]
        self.assertEqual(MODULE.chrome_tool_failures(events), [])

    def test_revision_is_read_from_the_view_or_its_observation(self) -> None:
        self.assertEqual(MODULE.revision_of({"revision": 12}), 12)
        self.assertEqual(MODULE.revision_of({"observation": {"revision": 41}}), 41)
        self.assertIsNone(MODULE.revision_of({}))


def view(pressed: str | None, revision: int = 1) -> dict:
    button: dict = {"role": "button", "name": MODULE.TOGGLE_NAME, "object_id": "o1"}
    if pressed is not None:
        button["state"] = {"enabled": "true", "pressed": pressed}
    return {"revision": revision, "objects": [
        button,
        {"role": "status", "object_id": "o3", "text": f"Browser cycle {revision}"},
    ]}


class ObservedChangeTests(unittest.TestCase):
    """A self-incrementing fixture must not be able to manufacture a PASS."""

    @staticmethod
    def changed(before: dict, after: dict) -> bool:
        first, second = MODULE.pressed_state(before), MODULE.pressed_state(after)
        return first is not None and second is not None and first != second

    def test_pressed_state_is_extracted_from_the_named_button(self) -> None:
        self.assertEqual(MODULE.pressed_state(view("false")), "false")
        self.assertEqual(MODULE.pressed_state(view("true")), "true")

    def test_missing_pressed_state_is_not_an_observation(self) -> None:
        self.assertIsNone(MODULE.pressed_state(view(None)))
        self.assertIsNone(MODULE.pressed_state({"objects": []}))

    def test_other_controls_are_never_mistaken_for_the_toggle(self) -> None:
        other = {"objects": [{"role": "button", "name": "Submit",
                              "state": {"pressed": "true"}}]}
        self.assertIsNone(MODULE.pressed_state(other))

    def test_revision_advance_alone_is_not_a_change(self) -> None:
        # The fixture's own push cycle moves revision 1 -> 47 with no click.
        self.assertFalse(self.changed(view("false", 1), view("false", 47)))

    def test_pressed_transition_is_a_change(self) -> None:
        self.assertTrue(self.changed(view("false", 1), view("true", 47)))


class SameTabVerdictTests(unittest.TestCase):
    """The pass rule itself: same-tab identity is hard, not advisory."""

    @staticmethod
    def verdict(execution_ids: list[str], opened: str = "77") -> bool:
        return bool(opened) and bool(execution_ids) and all(t == opened for t in execution_ids)

    def test_matching_single_tab_passes(self) -> None:
        self.assertTrue(self.verdict(["77"]))

    def test_no_chrome_execution_fails(self) -> None:
        self.assertFalse(self.verdict([]))

    def test_any_other_tab_fails(self) -> None:
        self.assertFalse(self.verdict(["77", "78"]))


if __name__ == "__main__":
    unittest.main()
