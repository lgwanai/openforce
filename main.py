import argparse
import sys
from src.core.db import init_db

def main():
    parser = argparse.ArgumentParser(description="OpenClaw Agent V6.5")
    parser.add_argument("--channel", type=str, choices=["cli", "api"], default="cli", help="Start the agent with specific channel")
    args = parser.parse_args()
    
    # Initialize Core Components
    init_db()
    
    if args.channel == "cli":
        from src.channels.cli import run_cli
        run_cli()
    elif args.channel == "api":
        print("API Channel is not fully implemented yet.")
        sys.exit(1)

if __name__ == "__main__":
    main()
