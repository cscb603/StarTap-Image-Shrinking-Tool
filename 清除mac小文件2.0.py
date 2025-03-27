import os
import tkinter as tk
from tkinter import messagebox
from tkinter import filedialog
import shutil

# 定义需要删除的文件和文件夹名称
MAC_SPECIFIC_FILES = ['.DS_Store']
# 添加 .system 到需要删除的文件夹列表中
MAC_SPECIFIC_FOLDERS = ['.fseventsd', '.Spotlight-V100', '.Trashes', '.TemporaryItems', '.system']

def clean_mac_files():
    # 让用户选择磁盘路径
    disk_path = filedialog.askdirectory()
    if not disk_path:
        return

    # 遍历指定路径下的所有文件和文件夹
    for root, dirs, files in os.walk(disk_path, topdown=False):
        # 处理文件
        for file in files:
            if file in MAC_SPECIFIC_FILES or file.startswith('._') or file.endswith('.prodadindex'):
                file_path = os.path.join(root, file)
                try:
                    os.remove(file_path)
                    print(f"已删除文件: {file_path}")
                except Exception as e:
                    print(f"删除文件 {file_path} 时出错: {e}")

        # 处理文件夹
        for dir_name in dirs:
            if dir_name in MAC_SPECIFIC_FOLDERS:
                dir_path = os.path.join(root, dir_name)
                try:
                    shutil.rmtree(dir_path)
                    print(f"已删除文件夹: {dir_path}")
                except Exception as e:
                    print(f"删除文件夹 {dir_path} 时出错: {e}")

    messagebox.showinfo("完成", "清理完成！")

# 创建主窗口
root = tk.Tk()
root.title("Mac 文件清理工具")

# 创建清理按钮
button = tk.Button(root, text="选择磁盘并一键清理", command=clean_mac_files)
button.pack(pady=50)

# 运行主循环
root.mainloop()
