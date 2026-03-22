import json
import re
from langchain_core.messages import AIMessage, SystemMessage, HumanMessage

def invoke_llm_with_tools(llm, tools, messages):
    """
    Wrapper for llm.invoke() that handles buggy API wrappers (like Tencent's Minimax).
    If the model is minimax or kimi, we inject tools into the prompt and parse XML natively
    to bypass the broken bind_tools implementation.
    """
    model_name = getattr(llm, "model_name", "")
    if "minimax" in model_name.lower() or "kimi" in model_name.lower():
        # Inject tools into System Prompt
        tools_desc = "\nYou are provided with these tools:\n<tools>\n"
        for t in tools:
            schema = t.args_schema.schema() if hasattr(t, "args_schema") and t.args_schema else {}
            tools_desc += json.dumps({"name": t.name, "description": t.description, "parameters": schema}, ensure_ascii=False) + "\n"
        tools_desc += "</tools>\nIf you need to call tools, please respond with <tool_calls></tool_calls> XML tags, and provide tool-name and json-object of arguments, following the format below:\n<tool_calls>\n{\"name\": <tool-name>, \"arguments\": <args-json-object>}\n...\n</tool_calls>\n"
        
        new_messages = []
        has_system = False
        for msg in messages:
            if isinstance(msg, SystemMessage):
                new_messages.append(SystemMessage(content=msg.content + tools_desc))
                has_system = True
            elif msg.type == "ai" and getattr(msg, "tool_calls", None):
                # Convert AIMessage with tool_calls into a plain AIMessage with XML content
                xml_calls = "<tool_calls>\n"
                for tc in msg.tool_calls:
                    xml_calls += json.dumps({"name": tc["name"], "arguments": tc["args"]}, ensure_ascii=False) + "\n"
                xml_calls += "</tool_calls>"
                content = (msg.content + "\n" + xml_calls).strip()
                new_messages.append(AIMessage(content=content))
            elif msg.type == "tool":
                # Convert ToolMessage into a plain HumanMessage
                content = f"Tool result for '{msg.name}':\n{msg.content}"
                new_messages.append(HumanMessage(content=content))
            else:
                # Strip out any residual tool_calls from AIMessage
                if isinstance(msg, AIMessage):
                    new_msg = AIMessage(content=msg.content)
                    new_messages.append(new_msg)
                else:
                    new_messages.append(msg)
        
        if not has_system:
            new_messages.insert(0, SystemMessage(content=tools_desc))
        
        response = llm.invoke(new_messages)
        
        # Parse XML from content
        content = response.content
        if isinstance(content, str) and "<tool_calls>" in content:
            tool_calls = []
            pattern = r"<tool_calls>(.*?)</tool_calls>"
            match = re.search(pattern, content, re.DOTALL)
            if match:
                xml_content = match.group(1).strip()
                
                # First try to see if it's a list of JSONs or just concatenated JSON objects
                 # Find all { ... } top-level objects
                 # This is a bit tricky, but since we expect {"name": ..., "arguments": ...} we can try to extract them
                json_strs = []
                brace_level = 0
                current_json = ""
                for char in xml_content:
                    if char == '{':
                        brace_level += 1
                    current_json += char
                    if char == '}':
                        brace_level -= 1
                        if brace_level == 0 and current_json.strip():
                            json_strs.append(current_json.strip())
                            current_json = ""
                            
                for j_str in json_strs:
                    try:
                        call_data = json.loads(j_str)
                        args_data = call_data.get("arguments", {})
                        if isinstance(args_data, str):
                            args_data = json.loads(args_data)
                        tool_calls.append({
                            "name": call_data.get("name"),
                            "args": args_data,
                            "id": f"call_{len(tool_calls)}"
                        })
                    except Exception as e:
                        import logging
                        logging.getLogger("OpenForce").error(f"Minimax manual parse error on object '{j_str}': {e}")
                        
                # Fallback for <tool> tags if any
                if not tool_calls and "<tool" in xml_content:
                    for line in xml_content.split('\n'):
                        line = line.strip()
                        if line.startswith("<tool"):
                            name_match = re.search(r'name=["\']([^"\']+)["\']', line)
                            args_match = re.search(r'"arguments":\s*(\{.*\})', line)
                            if name_match:
                                name = name_match.group(1)
                                args_str = args_match.group(1) if args_match else "{}"
                                try:
                                    tool_calls.append({
                                        "name": name,
                                        "args": json.loads(args_str),
                                        "id": f"call_{len(tool_calls)}"
                                    })
                                except Exception:
                                    pass
            
            if tool_calls:
                response.tool_calls = tool_calls
                # Strip out the <tool_calls> block so it's not shown to the user
                response.content = re.sub(r"<tool_calls>.*?</tool_calls>", "", content, flags=re.DOTALL).strip()
                
        return response
    else:
        # Standard flow
        print(f"DEBUG: invoking model {model_name} with messages:")
        for m in messages:
            print(f"  {m.type}: {m.content[:50]}")
            if hasattr(m, 'tool_calls'):
                print(f"    tool_calls: {m.tool_calls}")
        llm_with_tools = llm.bind_tools(tools)
        try:
            response = llm_with_tools.invoke(messages)
        except Exception as e:
            print(f"ERROR calling {model_name}: {e}")
            print("Messages were:")
            for m in messages:
                print(m.dict())
            raise
        return ensure_tool_calls_parsed(response)

def ensure_tool_calls_parsed(response: AIMessage) -> AIMessage:
    """
    Ensure tool calls from different LLM providers (Minimax, Deepseek, etc.)
    are properly parsed into response.tool_calls.
    """
    # Fix for Minimax bug where arguments string is sometimes cut off (missing closing brace/quote)
    if getattr(response, "tool_calls", None):
        for tc in response.tool_calls:
            if not isinstance(tc.get("args"), dict):
                try:
                    if not tc["args"]:
                        tc["args"] = {}
                    elif isinstance(tc["args"], str):
                        # Attempt to fix truncated json strings from Minimax
                        args_str = tc["args"].strip()
                        if args_str.startswith("{") and not args_str.endswith("}"):
                            # It's truncated.
                            if args_str.count('"') % 2 != 0:
                                args_str += '"'
                            args_str += "}"
                        tc["args"] = json.loads(args_str)
                except Exception:
                    tc["args"] = {}
        return response

    raw_tools = getattr(response, "additional_kwargs", {}).get("tool_calls", [])
    
    # Check invalid_tool_calls from Langchain
    if not raw_tools and getattr(response, "invalid_tool_calls", None):
        for invalid_tc in response.invalid_tool_calls:
            # Langchain stores it as {'name': '...', 'args': '...', 'id': '...'}
            raw_tools.append({
                "function": {
                    "name": invalid_tc.get("name", ""),
                    "arguments": invalid_tc.get("args", "{}")
                },
                "id": invalid_tc.get("id", "call_1")
            })
    
    # Langchain sometimes parses invalid JSON but stores the raw string somewhere else, or drops it.
    # We must intercept it BEFORE Langchain completely discards the invalid tool call.
    # Actually, if Langchain completely discards it from both tool_calls and additional_kwargs, 
    # we cannot recover it here without a custom parser in ChatOpenAI.
    
    # Check if the model returned raw tool calls in message kwargs but they weren't parsed
    if not raw_tools and hasattr(response, "response_metadata"):
        pass
        
    # Extra check for some Minimax formats
    if not raw_tools and getattr(response, "tool_calls", None) is None:
        if "function_call" in response.additional_kwargs:
             pass
        # Minimax vLLM response might have tool_calls in additional_kwargs but it's a string
        if "tool_calls" in response.additional_kwargs:
            pass
    
    # Minimax specific format (sometimes it puts function_call inside additional_kwargs)
    if not raw_tools and "function_call" in getattr(response, "additional_kwargs", {}):
        fn_call = response.additional_kwargs["function_call"]
        if fn_call:
            raw_tools = [{"function": fn_call, "id": "call_1"}]
            
    # Also check choices[0].message.tool_calls manually if response has a raw _raw_response object
    if not raw_tools and hasattr(response, "response_metadata"):
        # LangChain sometimes just loses the tool calls entirely if they fail some strict validation.
        pass
            
    # Deepseek or other format
    if not raw_tools and hasattr(response, "tool_calls") and response.tool_calls:
         raw_tools = response.tool_calls
         
    # If the provider simply sets finish_reason="tool_calls" but provides no tool_calls field in choices
    # we have to guess or return empty. We can't fabricate a tool call out of thin air if the API didn't return one.
    if not raw_tools and not getattr(response, "tool_calls", None):
        # We can't recover it here, let the caller handle it based on finish_reason
        return response
         
    if raw_tools:
        parsed_tools = []
        for t in raw_tools:
            try:
                args_str = t["function"].get("arguments", "{}")
                if not args_str:
                    args_str = "{}"
                
                # Attempt to fix truncated json strings from Minimax
                if isinstance(args_str, str):
                    args_str = args_str.strip()
                    if args_str.startswith("{") and not args_str.endswith("}"):
                        # Try to close strings if open
                        if args_str.count('"') % 2 != 0:
                            args_str += '"'
                        args_str += "}"
                    
                    try:
                        args = json.loads(args_str)
                    except json.JSONDecodeError:
                        # If it STILL fails, just fallback to empty dict
                        args = {}
                else:
                    args = args_str
                    
                parsed_tools.append({
                    "name": t["function"]["name"],
                    "args": args,
                    "id": t.get("id", "call_1")
                })
            except Exception as e:
                import logging
                logging.getLogger("OpenForce").error(f"Failed to parse tool call args: {e}, args: {t.get('function', {}).get('arguments')}")
                pass
        
        if parsed_tools:
            response.additional_kwargs["tool_calls"] = raw_tools
            response.tool_calls = parsed_tools

    return response
