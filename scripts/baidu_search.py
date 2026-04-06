#!/usr/bin/env python3
"""
Baidu AI Search Script using DashScope API.

This script uses Alibaba's Qwen model with web search capability.

Usage:
    python baidu_search.py '{"query": "搜索关键词"}'

Environment:
    DASHSCOPE_API_KEY - API key for DashScope
"""

import os
import sys
import json


def search_with_qwen(query: str) -> str:
    """Search using Qwen model with search capability."""
    try:
        import dashscope
        from dashscope import Generation

        api_key = os.environ.get("DASHSCOPE_API_KEY", "")
        if not api_key:
            return json.dumps({"error": "DASHSCOPE_API_KEY not set"}, ensure_ascii=False)

        dashscope.api_key = api_key

        # Use Qwen-turbo which supports web search
        # Reference: https://help.aliyun.com/zh/dashscope/developer-reference/web-search
        messages = [
            {"role": "system", "content": "你是一个有帮助的助手，可以搜索网络信息。请提供准确、有用的回答。"},
            {"role": "user", "content": query}
        ]

        response = Generation.call(
            model='qwen-turbo',
            messages=messages,
            search_info={"enable_search": True},
            result_format='message'
        )

        if response.status_code == 200:
            # Extract the response content
            content = response.output.choices[0].message.content
            result = {
                "query": query,
                "answer": content,
                "status": "success"
            }
            return json.dumps(result, ensure_ascii=False, indent=2)
        else:
            return json.dumps({
                "error": f"API error: {response.code} - {response.message}",
                "query": query
            }, ensure_ascii=False)

    except ImportError:
        return json.dumps({
            "error": "dashscope package not installed. Run: pip install dashscope"
        }, ensure_ascii=False)
    except Exception as e:
        return json.dumps({"error": str(e), "query": query}, ensure_ascii=False)


def search_with_application(query: str) -> str:
    """
    Alternative: Use DashScope Application (AppBuilder) for search.

    This method uses a pre-configured application that has web search enabled.
    Requires OPENFORCE_DASHSCOPE_APP_ID environment variable.
    """
    try:
        import dashscope
        from dashscope import Application

        api_key = os.environ.get("DASHSCOPE_API_KEY", "")
        app_id = os.environ.get("OPENFORCE_DASHSCOPE_APP_ID", "")

        if not api_key:
            return json.dumps({"error": "DASHSCOPE_API_KEY not set"}, ensure_ascii=False)
        if not app_id:
            return search_with_qwen(query)  # Fallback to qwen

        dashscope.api_key = api_key

        response = Application.call(
            app_id=app_id,
            prompt=query
        )

        if response.status_code == 200:
            return json.dumps({
                "query": query,
                "answer": response.output.text,
                "status": "success"
            }, ensure_ascii=False, indent=2)
        else:
            return search_with_qwen(query)  # Fallback

    except Exception:
        return search_with_qwen(query)


def main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Usage: python baidu_search.py '{\"query\": \"搜索词\"}'"}, ensure_ascii=False))
        sys.exit(1)

    try:
        args = json.loads(sys.argv[1])
        query = args.get("query", "")
    except json.JSONDecodeError:
        query = sys.argv[1]

    if not query:
        print(json.dumps({"error": "No query provided"}, ensure_ascii=False))
        sys.exit(1)

    # Try Application first, then fallback to qwen-turbo
    result = search_with_application(query)
    print(result)


if __name__ == "__main__":
    main()
