import re
import pytest
from agent02.agent import run_tool_agent, make_tools, tool_calls, TOOL_DEFS
from agent02.core import calc


def test_calc():
    assert calc("2+3*(4-1)") == 11
    with pytest.raises(Exception):
        calc("alert(1)")


def test_tool_loop_runs_native_tool_calls_then_answers():
    replies = [{"tool_calls": [{"name": "calculator", "arguments": {"expression": "6*7"}}]}, {"response": "42"}]
    def ai(_m, inp):
        if len(replies) == 1:
            assert re.search("calculator: 42", inp["messages"][-1]["content"])
        return replies.pop(0)
    r = run_tool_agent(ai, "6*7?", make_tools(None))
    assert r["answer"] == "42" and r["trace"][0]["result"] == "42"


def test_parses_tool_calls_emitted_as_text():
    r = {"response": ['{"type":"function","name":"calculator","parameters":{"expression":"2+2"}}']}
    assert tool_calls(r) == [{"name": "calculator", "arguments": {"expression": "2+2"}}]
    assert tool_calls({"response": "Hello there"}) == []


def test_weather_tool_with_mocked_http():
    def f(u, h=None):
        if "geocoding" in u:
            return True, {"results": [{"name": "Austin", "country": "US", "latitude": 1, "longitude": 2}]}
        return True, {"current": {"temperature_2m": 20, "wind_speed_10m": 5}}
    assert re.search("Austin, US: 20°C", make_tools(f)["get_weather"]({"city": "Austin"}))
