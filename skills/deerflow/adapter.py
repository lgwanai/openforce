#!/usr/bin/env python3
"""Protocol adapter: stdin JSON → deerflow tool → stdout JSON. No langchain needed."""
import sys, json, os

SKILL_ROOT = os.path.dirname(os.path.abspath(__file__))
DEERFLOW_CONFIG = os.path.join(SKILL_ROOT, "config.yaml")

def load_api_key(tool_name):
    try:
        with open(DEERFLOW_CONFIG) as f:
            lines = f.readlines()
        for i, line in enumerate(lines):
            if f"name: {tool_name}" in line:
                for j in range(i, min(i+5, len(lines))):
                    if "api_key:" in lines[j]:
                        return lines[j].split("api_key:")[1].strip()
    except: pass
    return os.environ.get(f"{tool_name.upper()}_API_KEY", "")

def tool_web_search(args):
    import urllib.request
    api_key = load_api_key("web_search")
    if not api_key: return {"error": "TAVILY_API_KEY not configured"}
    query = args.get("query",""); n = args.get("max_results",3)
    req = urllib.request.Request("https://api.tavily.com/search",
        data=json.dumps({"api_key":api_key,"query":query,"max_results":n,"search_depth":"basic"}).encode(),
        headers={"Content-Type":"application/json"})
    with urllib.request.urlopen(req, timeout=10) as r:
        body = json.loads(r.read())
        results = [{"title":x.get("title",""),"url":x.get("url",""),"content":x.get("content","")[:500]} for x in body.get("results",[])][:n]
        return {"results": results}

def tool_web_fetch(args):
    import urllib.request
    api_key = load_api_key("web_fetch")
    if not api_key: return {"error": "JINA_API_KEY not configured"}
    url = args.get("url","")
    req = urllib.request.Request(f"https://r.jina.ai/{url}", headers={"Authorization":f"Bearer {api_key}"})
    with urllib.request.urlopen(req, timeout=15) as r:
        return {"content": r.read().decode()[:4000]}

TOOLS = {"web_search": tool_web_search, "web_fetch": tool_web_fetch}

if __name__ == "__main__":
    try:
        req = json.load(sys.stdin)
        tool = req.get("tool",""); args = req.get("args",{})
        if tool not in TOOLS:
            print(json.dumps({"error":f"unknown tool: {tool}","available":list(TOOLS.keys())}))
            sys.exit(1)
        print(json.dumps(TOOLS[tool](args), ensure_ascii=False))
    except Exception as e:
        print(json.dumps({"error":str(e)}))
        sys.exit(1)
