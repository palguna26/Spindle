"""Spindle plugin for Hermes session identity."""
import os
import subprocess
import time

_SOURCE = "spindle:hermes"
_AGENT = "hermes"
_PLATFORMS = {"cli", "tui", "desktop", "acp"}

def _send(session_id, source):
    pane = os.environ.get("SPINDLE_PANE_ID") or os.environ.get("HERDR_PANE_ID")
    if not pane or (os.environ.get("SPINDLE_ENV") or os.environ.get("HERDR_ENV")) != "1": return
    command = [os.environ.get("SPINDLE_BIN_PATH") or os.environ.get("HERDR_BIN_PATH") or "spindle", "pane", "report-agent-session", pane, "--source", _SOURCE, "--agent", _AGENT, "--seq", str(time.time_ns()), "--agent-session-id", session_id, "--session-start-source", source]
    try: subprocess.run(command, check=False, timeout=1, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    except Exception: pass

def _report(source, **kwargs):
    if kwargs.get("platform") in _PLATFORMS and isinstance(kwargs.get("session_id"), str) and kwargs["session_id"]: _send(kwargs["session_id"], source)
def register(ctx):
    ctx.register_hook("on_session_start", lambda **kw: _report("startup", **kw))
    ctx.register_hook("on_session_reset", lambda **kw: _report("new", **kw))
    ctx.register_hook("pre_llm_call", lambda **kw: _report("resume", **kw) if kw.get("platform") == "cli" else None)
