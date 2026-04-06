"""
Documentation generation tools for Libu2 Agent.

This module provides tools for extracting and generating documentation
from Python source code using AST parsing.
"""

import ast
import json
import os
from pathlib import Path
from typing import Dict, List, Optional, Any


def extract_docstrings(filepath: str) -> str:
    """
    Extract all docstrings from a Python file using AST.

    Args:
        filepath: Path to Python file (relative to project root)

    Returns:
        JSON string containing extracted docstrings organized by type
    """
    try:
        # Resolve path relative to project root
        project_root = get_project_root()
        absolute_path = (project_root / filepath).resolve()

        # Security: Ensure path is within project root
        try:
            absolute_path.relative_to(project_root.resolve())
        except ValueError:
            return json.dumps({"error": f"Path escapes project root: {filepath}"})

        if not absolute_path.exists():
            return json.dumps({"error": f"File not found: {filepath}"})

        if not absolute_path.suffix == ".py":
            return json.dumps({"error": f"Not a Python file: {filepath}"})

        # Parse the file
        source = absolute_path.read_text(encoding="utf-8")
        tree = ast.parse(source)

        docstrings: Dict[str, Any] = {
            "module": None,
            "classes": [],
            "functions": [],
            "methods": []
        }

        # Extract module docstring
        module_doc = ast.get_docstring(tree)
        if module_doc:
            docstrings["module"] = module_doc

        # Walk the AST
        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef):
                class_doc = ast.get_docstring(node)
                if class_doc:
                    docstrings["classes"].append({
                        "name": node.name,
                        "docstring": class_doc,
                        "lineno": node.lineno
                    })

            elif isinstance(node, ast.FunctionDef) or isinstance(node, ast.AsyncFunctionDef):
                func_doc = ast.get_docstring(node)

                # Determine if this is a method (inside a class) or a function
                is_method = False
                for parent in ast.walk(tree):
                    if isinstance(parent, ast.ClassDef):
                        for child in parent.body:
                            if child is node:
                                is_method = True
                                break

                if func_doc:
                    doc_info = {
                        "name": node.name,
                        "docstring": func_doc,
                        "lineno": node.lineno,
                        "args": [arg.arg for arg in node.args.args]
                    }

                    if is_method:
                        docstrings["methods"].append(doc_info)
                    else:
                        docstrings["functions"].append(doc_info)

        return json.dumps(docstrings, ensure_ascii=False, indent=2)

    except SyntaxError as e:
        return json.dumps({"error": f"Syntax error in {filepath}: {str(e)}"})
    except Exception as e:
        return json.dumps({"error": f"Error extracting docstrings: {str(e)}"})


def generate_doc(module_name: str, output_format: str = "markdown") -> str:
    """
    Generate documentation for a module.

    Args:
        module_name: Module name or path (e.g., "src.tools.base")
        output_format: Output format ("markdown" or "json")

    Returns:
        Generated documentation as string
    """
    try:
        # Convert module name to file path
        project_root = get_project_root()

        # Try different possible locations
        possible_paths = [
            project_root / module_name.replace(".", "/") / "__init__.py",
            project_root / (module_name.replace(".", "/") + ".py"),
        ]

        file_path = None
        for path in possible_paths:
            if path.exists():
                file_path = path
                break

        if not file_path:
            return json.dumps({"error": f"Module not found: {module_name}"})

        # Extract docstrings
        relative_path = str(file_path.relative_to(project_root))
        docstrings_json = extract_docstrings(relative_path)
        docstrings = json.loads(docstrings_json)

        if "error" in docstrings:
            return json.dumps(docstrings)

        if output_format == "json":
            return json.dumps(docstrings, ensure_ascii=False, indent=2)

        # Generate markdown
        md_lines = []
        md_lines.append(f"# {module_name}\n")

        if docstrings["module"]:
            md_lines.append(f"{docstrings['module']}\n")

        # Classes
        if docstrings["classes"]:
            md_lines.append("## Classes\n")
            for cls in docstrings["classes"]:
                md_lines.append(f"### {cls['name']}\n")
                md_lines.append(f"{cls['docstring']}\n")

        # Functions
        if docstrings["functions"]:
            md_lines.append("## Functions\n")
            for func in docstrings["functions"]:
                args_str = ", ".join(func["args"]) if func["args"] else ""
                md_lines.append(f"### {func['name']}({args_str})\n")
                md_lines.append(f"{func['docstring']}\n")

        # Methods
        if docstrings["methods"]:
            md_lines.append("## Methods\n")
            for method in docstrings["methods"]:
                args_str = ", ".join(method["args"]) if method["args"] else ""
                md_lines.append(f"### {method['name']}({args_str})\n")
                md_lines.append(f"{method['docstring']}\n")

        return "\n".join(md_lines)

    except Exception as e:
        return json.dumps({"error": f"Error generating documentation: {str(e)}"})


def format_markdown(content: str, style: str = "standard") -> str:
    """
    Format markdown content according to a specific style.

    Args:
        content: Markdown content to format
        style: Formatting style ("standard", "compact", "expanded")

    Returns:
        Formatted markdown content
    """
    try:
        lines = content.split("\n")

        if style == "compact":
            # Remove blank lines between items
            formatted_lines = []
            prev_blank = False
            for line in lines:
                is_blank = line.strip() == ""
                if is_blank and prev_blank:
                    continue
                formatted_lines.append(line)
                prev_blank = is_blank
            return "\n".join(formatted_lines)

        elif style == "expanded":
            # Add extra spacing around headers
            formatted_lines = []
            for line in lines:
                if line.startswith("#") and formatted_lines:
                    if formatted_lines[-1].strip() != "":
                        formatted_lines.append("")
                formatted_lines.append(line)
            return "\n".join(formatted_lines)

        else:  # standard
            # Ensure single blank line between sections
            formatted_lines = []
            prev_was_header = False
            for line in lines:
                is_header = line.startswith("#")
                if prev_was_header and line.strip() == "":
                    continue
                formatted_lines.append(line)
                prev_was_header = is_header
            return "\n".join(formatted_lines)

    except Exception as e:
        return f"Error formatting markdown: {str(e)}"


def create_readme(title: str, description: str, sections: str = "") -> str:
    """
    Create a README file content.

    Args:
        title: Project title
        description: Project description
        sections: Additional sections as markdown (optional)

    Returns:
        README content as string
    """
    try:
        lines = [
            f"# {title}",
            "",
            f"{description}",
            ""
        ]

        if sections:
            lines.append(sections)
            if not sections.endswith("\n"):
                lines.append("")

        # Add standard sections if not provided
        if "## Installation" not in sections:
            lines.extend([
                "## Installation",
                "",
                "```bash",
                "pip install <package-name>",
                "```",
                ""
            ])

        if "## Usage" not in sections:
            lines.extend([
                "## Usage",
                "",
                "```python",
                "import <module>",
                "",
                "# Add usage examples here",
                "```",
                ""
            ])

        if "## License" not in sections:
            lines.extend([
                "## License",
                "",
                "MIT License",
                ""
            ])

        return "\n".join(lines)

    except Exception as e:
        return f"Error creating README: {str(e)}"


def get_project_root() -> Path:
    """Get the project root directory."""
    current = Path.cwd()
    while current != current.parent:
        if (current / "openforce.db").exists() or (current / ".git").exists():
            return current
        current = current.parent
    return Path.cwd()
