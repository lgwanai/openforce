import json
from typing import Dict, Any

def spawn_agent(role: str, goal: str, acceptance_criteria: str) -> str:
    """实例化并派发任务给特定的子 Agent"""
    # This will be intercepted by the orchestrator framework
    return json.dumps({
        "action": "SPAWN",
        "role": role,
        "goal": goal,
        "acceptance_criteria": acceptance_criteria
    })

def update_task_queue(task_id: str, status: str) -> str:
    """更新宏观状态机中的子任务状态"""
    return json.dumps({
        "action": "UPDATE_QUEUE",
        "task_id": task_id,
        "status": status
    })

def report_status(summary: str) -> str:
    """尚书省向上汇报成果"""
    return json.dumps({
        "action": "REPORT",
        "summary": summary
    })
