import json
import sqlite3
from typing import Dict, Any, List, Optional
from datetime import datetime
from pydantic import BaseModel, Field

DB_PATH = "openforce.db"

class TaskRecord(BaseModel):
    task_id: str
    owner_user_id: str
    conversation_id: str
    thread_id: str
    original_req: str
    status: str
    goals: List[str] = []
    criteria: List[str] = []
    budget: Dict[str, Any] = {}
    idempotency_key: Optional[str] = None
    operation_id: Optional[str] = None
    trust_level: str = "Untrusted"
    taint_source: str = "user_free_text"
    evidence_ref: Optional[str] = None
    pending_approval_id: Optional[str] = None
    pending_input_id: Optional[str] = None
    approval_action_hash: Optional[str] = None
    approval_snapshot_id: Optional[str] = None
    approver_id: Optional[str] = None
    checkpoints: List[Dict[str, Any]] = []
    allowed_commands: List[str] = []
    outbox_refs: List[str] = []
    created_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())
    updated_at: str = Field(default_factory=lambda: datetime.utcnow().isoformat())

def init_db():
    conn = sqlite3.connect(DB_PATH)
    c = conn.cursor()
    c.execute('''
        CREATE TABLE IF NOT EXISTS tasks (
            task_id TEXT PRIMARY KEY,
            owner_user_id TEXT,
            conversation_id TEXT,
            thread_id TEXT,
            data JSON,
            status TEXT,
            created_at TEXT,
            updated_at TEXT
        )
    ''')
    
    c.execute('''
        CREATE TABLE IF NOT EXISTS active_tasks (
            owner_user_id TEXT PRIMARY KEY,
            task_id TEXT
        )
    ''')
    
    c.execute('''
        CREATE TABLE IF NOT EXISTS used_nonces (
            nonce TEXT PRIMARY KEY,
            consumed_at TEXT
        )
    ''')
    conn.commit()
    conn.close()

def save_task(task: TaskRecord):
    conn = sqlite3.connect(DB_PATH)
    c = conn.cursor()
    task.updated_at = datetime.utcnow().isoformat()
    data_json = task.model_dump_json()
    
    c.execute('''
        INSERT INTO tasks (task_id, owner_user_id, conversation_id, thread_id, data, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(task_id) DO UPDATE SET
            data=excluded.data,
            status=excluded.status,
            updated_at=excluded.updated_at
    ''', (task.task_id, task.owner_user_id, task.conversation_id, task.thread_id, data_json, task.status, task.created_at, task.updated_at))
    conn.commit()
    conn.close()

def get_task(task_id: str) -> Optional[TaskRecord]:
    conn = sqlite3.connect(DB_PATH)
    c = conn.cursor()
    c.execute('SELECT data FROM tasks WHERE task_id = ?', (task_id,))
    row = c.fetchone()
    conn.close()
    if row:
        data = json.loads(row[0])
        return TaskRecord(**data)
    return None

def set_active_task(owner_user_id: str, task_id: Optional[str]):
    """
    Session Gate concurrency control: must be fast.
    """
    conn = sqlite3.connect(DB_PATH, isolation_level="EXCLUSIVE")
    c = conn.cursor()
    try:
        if task_id is None:
            c.execute('DELETE FROM active_tasks WHERE owner_user_id = ?', (owner_user_id,))
        else:
            c.execute('''
                INSERT INTO active_tasks (owner_user_id, task_id)
                VALUES (?, ?)
                ON CONFLICT(owner_user_id) DO UPDATE SET task_id=excluded.task_id
            ''', (owner_user_id, task_id))
        conn.commit()
    finally:
        conn.close()

def get_active_task(owner_user_id: str) -> Optional[str]:
    conn = sqlite3.connect(DB_PATH)
    c = conn.cursor()
    c.execute('SELECT task_id FROM active_tasks WHERE owner_user_id = ?', (owner_user_id,))
    row = c.fetchone()
    conn.close()
    return row[0] if row else None

def consume_nonce(nonce: str) -> bool:
    """Atomic nonce consumption"""
    conn = sqlite3.connect(DB_PATH, isolation_level="EXCLUSIVE")
    c = conn.cursor()
    try:
        c.execute('INSERT INTO used_nonces (nonce, consumed_at) VALUES (?, ?)', 
                  (nonce, datetime.utcnow().isoformat()))
        conn.commit()
        return True
    except sqlite3.IntegrityError:
        return False
    finally:
        conn.close()

# Initialize DB on import
init_db()
