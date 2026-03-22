import logging
import os
from logging.handlers import RotatingFileHandler

LOG_DIR = os.path.join(os.path.dirname(__file__), "../../logs")
os.makedirs(LOG_DIR, exist_ok=True)

# 3.8.12 Log Full Trace
# Create a logger
logger = logging.getLogger("OpenForce")
logger.setLevel(logging.DEBUG)

# File handler with rotation (500MB limit per file, max 10 backups -> 5GB quota)
file_handler = RotatingFileHandler(
    os.path.join(LOG_DIR, "openforce.log"),
    maxBytes=500 * 1024 * 1024,
    backupCount=10,
    encoding="utf-8"
)
file_handler.setLevel(logging.DEBUG)

# Formatter
formatter = logging.Formatter(
    "%(asctime)s | %(levelname)s | [%(name)s] %(message)s"
)
file_handler.setFormatter(formatter)

logger.addHandler(file_handler)

def get_logger(name: str):
    return logger.getChild(name)

class SecretScanner:
    """
    3.8.8 Context Redaction Protocol
    """
    @staticmethod
    def redact(text: str) -> str:
        if not isinstance(text, str):
            return text
        # Simple redaction for demonstration (e.g. sk-...)
        import re
        # Redact generic sk-xxx keys
        redacted = re.sub(r"sk-[a-zA-Z0-9_-]{20,}", "[REDACTED_SECRET_HASH]", text)
        # Redact tencent cloud keys (sk-[...])
        redacted = re.sub(r"sk-[a-zA-Z0-9]{30,}", "[REDACTED_SECRET_HASH]", redacted)
        return redacted

def log_audit_event(task_id: str, action: str, data: str):
    """
    Logs an audit event after passing it through the SecretScanner.
    """
    safe_data = SecretScanner.redact(str(data))
    logger.info(f"TaskID: {task_id} | Action: {action} | Data: {safe_data}")
