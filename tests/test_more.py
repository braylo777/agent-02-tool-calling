import pytest
from agent02.agent import run_tool_agent, make_tools, tool_calls, TOOL_DEFS
from agent02.core import calc


def test_tool_defs_valid():
    assert [t["name"] for t in TOOL_DEFS] == ["calculator", "get_weather", "wikipedia"]
    for t in TOOL_DEFS:
        assert t["parameters"]["type"] == "object" and t["parameters"]["required"]


def test_calc_edge_cases():
    assert make_tools(None)["calculator"]({"expression": "0.18*240"}) == "43.2"
    with pytest.raises(Exception):
        calc("2**")


def test_string_arguments_json_parsed():
    assert tool_calls({"tool_calls": [{"name": "wikipedia", "arguments": '{"topic":"Rust"}'}]}) == [{"name": "wikipedia", "arguments": {"topic": "Rust"}}]


def test_bad_arguments_become_empty_and_nameless_dropped():
    assert tool_calls({"tool_calls": [{"name": "calculator", "arguments": "{oops"}, {"arguments": {}}]}) == [{"name": "calculator", "arguments": {}}]


def test_single_object_in_response_text():
    assert tool_calls({"response": 'Sure {"name":"get_weather","arguments":{"city":"Oslo"}}'}) == [{"name": "get_weather", "arguments": {"city": "Oslo"}}]


def test_weather_unknown_city():
    assert make_tools(lambda u, h=None: (True, {"results": []}))["get_weather"]({"city": "Nowhere"}) == "No city named Nowhere"


def test_wikipedia_truncated_and_404():
    assert len(make_tools(lambda u, h=None: (True, {"extract": "x" * 900}))["wikipedia"]({"topic": "A B"})) == 600
    assert make_tools(lambda u, h=None: (False, {}))["wikipedia"]({"topic": "Zzz"}) == "No article for Zzz"


def test_wikipedia_spaces_to_underscores():
    seen = []
    make_tools(lambda u, h=None: (seen.append(u), (True, {"extract": "e"}))[1])["wikipedia"]({"topic": "Ada Lovelace"})
    assert seen[0].endswith("Ada_Lovelace")


def test_parallel_calls_and_unknown_tool():
    replies = [{"tool_calls": [{"name": "calculator", "arguments": {"expression": "2+2"}}, {"name": "nope", "arguments": {}}]}, {"response": "done"}]
    r = run_tool_agent(lambda m, i: replies.pop(0), "q", make_tools(lambda u, h=None: (True, {})))
    assert [t["result"] for t in r["trace"]] == ["4", "unknown tool nope"] and r["answer"] == "done"


def test_step_limit():
    k = iter(range(100))
    r = run_tool_agent(lambda m, i: {"tool_calls": [{"name": "calculator", "arguments": {"expression": f"{next(k)}+1"}}]}, "q", make_tools(None), 2)
    assert r["answer"] == "Step limit reached." and len(r["trace"]) == 2


def test_loop_guard_forces_tool_free_answer():
    seen = []
    call = {"tool_calls": [{"name": "calculator", "arguments": {"expression": "1+1"}}]}
    replies = [call, call, {"response": "2"}]
    def ai(_m, inp):
        seen.append("tools" in inp)
        return replies.pop(0)
    r = run_tool_agent(ai, "q", make_tools(None))
    assert r["answer"] == "2" and len(r["trace"]) == 1 and seen == [True, True, False]
