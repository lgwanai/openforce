import hashlib
import time
from typing import Dict, Any, Optional

class TaintEngine:
    """
    Taint propagation and sensitive tool verification protocol.
    trust_level: Trusted | Derived | Untrusted
    taint_source: web | search | upload | user_free_text | internal
    """
    
    @staticmethod
    def get_trust_level(sources: list[str]) -> str:
        if "web" in sources or "upload" in sources:
            return "Untrusted"
        if "user_free_text" in sources:
            return "Derived"
        return "Trusted"
        
    @staticmethod
    def check_tool_call(tool_name: str, args: Dict[str, Any], trust_level: str) -> bool:
        """
        Hard constraint on execution gateway.
        """
        high_risk_tools = ["execute_command", "delete_file", "write_api"]
        if tool_name in high_risk_tools:
            # Must require human approval, no exemptions.
            return False
            
        return True

def generate_approval_token(owner_user_id: str, task_id: str, approval_id: str, action_hash: str, exp: int, nonce: str, channel_binding_hash: str) -> str:
    raw = f"{owner_user_id}:{task_id}:{approval_id}:{action_hash}:{exp}:{nonce}:{channel_binding_hash}"
    # In a real system, this would be an HMAC with a secret key
    return hashlib.sha256(raw.encode()).hexdigest()

def verify_approval_token(token: str, owner_user_id: str, task_id: str, approval_id: str, action_hash: str, exp: int, nonce: str, channel_binding_hash: str) -> bool:
    if time.time() > exp:
        return False
    expected = generate_approval_token(owner_user_id, task_id, approval_id, action_hash, exp, nonce, channel_binding_hash)
    return token == expected
