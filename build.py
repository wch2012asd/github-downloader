"""PyInstaller 打包脚本：将 GitHub 下载器打包成单文件 EXE。

用法: python build.py
"""

import subprocess
import sys
import os

def main():
    if not os.path.exists("main.py"):
        print("错误: 找不到 main.py，请在项目目录中运行")
        sys.exit(1)

    cmd = [
        sys.executable, "-m", "PyInstaller",
        "--onefile",
        "--windowed",
        "--name", "GitHubDownloader",
        "--icon", "assets/app_icon.ico",
        "--noconfirm",
        # customtkinter 的主题 JSON 与 assets/ 图标是数据文件，需显式收集
        "--collect-data", "customtkinter",
        "--add-data", "assets;assets",
        "main.py",
    ]

    print("开始打包...")
    print(" ".join(cmd))
    subprocess.run(cmd, check=True)
    print("\n打包完成! EXE 文件在 dist/ 目录中")

if __name__ == "__main__":
    main()
