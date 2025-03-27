# Mac-Storage-Junk-Files-Remover
U盘或者移动硬盘在MAC电脑上使用过后，会多出来很多小文件，比如.Trashes或者一些 ._开头的文件，这工具可以一键删除这些。
中文说明
项目简介

此项目是一个用于自动清理U盘或移动硬盘中从Mac电脑系统带过来的小文件的工具。这些小文件通常是Mac系统为了管理文件和文件夹而创建的，但在其他系统中可能没有实际用途，甚至会占用存储空间。该工具可以帮助你轻松地删除这些文件和文件夹。

win系统电脑64为可以直接下载我编译好的exe文件使用，或者你可以使用py命令行运行。
需要安装的库

此项目使用了Python的标准库，因此不需要额外安装任何库。所需的库包括：

   1 os：用于操作系统相关的功能，如文件和文件夹的遍历和删除。
   2 tkinter：用于创建图形用户界面（GUI），方便用户选择磁盘路径。
   3 shutil：用于高级的文件操作，如递归删除文件夹。

使用说明

   1 运行脚本：确保你已经安装了Python环境，然后运行清除mac小文件2.0.py脚本。
   2 选择磁盘：脚本运行后，会弹出一个图形界面，点击“选择磁盘并一键清理”按钮。
   3 选择路径：在弹出的文件选择对话框中，选择你要清理的U盘或移动硬盘的根目录。
   4 开始清理：选择好路径后，点击“确定”按钮，脚本会开始遍历指定路径下的所有文件和文件夹，并删除所有Mac特定的文件和文件夹。
   5 完成提示：清理完成后，会弹出一个消息框，提示“清理完成！”。

English Description  英文描述
Project Introduction  项目介绍

This project is a tool designed to automatically clean up small files brought over from a Mac computer system on a USB flash drive or external hard drive. These small files are typically created by the Mac system for file and folder management, but they may have no practical use on other systems and can even take up storage space. This tool can help you easily delete these files and folders.
该项目是一个工具，旨在自动清理从 Mac 计算机系统通过 USB 闪存驱动器或外部硬盘传输过来的小文件。这些小文件通常是 Mac 系统为了文件和文件夹管理而创建的，但在其他系统中可能没有实际用途，甚至会占用存储空间。此工具可以帮助您轻松删除这些文件和文件夹。
Libraries to Install 

This project uses Python's standard libraries, so no additional libraries need to be installed. The required libraries include: 

   1 os: Used for operating system-related functions, such as traversing and deleting files and folders. 
   2 tkinter: Used to create a graphical user interface (GUI) for users to select the disk path easily.
   3 tkinter : 用于创建图形用户界面（GUI），使用户能够轻松选择磁盘路径。
   4 shutil: Used for advanced file operations, such as recursively deleting folders.
   5 shutil : 用于执行高级文件操作，例如递归删除文件夹。

Usage Instructions  使用说明

    Run the script: Make sure you have a Python environment installed, then run the 清除mac小文件2.0.py script.
    运行脚本：确保已安装 Python 环境，然后运行 清除mac小文件2.0.py 脚本。
    Select the disk: After the script runs, a graphical interface will pop up. Click the "Select Disk and Clean with One Click" button.
    选择磁盘：脚本运行后，将弹出一个图形界面。点击“一键选择磁盘并清理”按钮。
    Choose the path: In the file selection dialog that appears, select the root directory of the USB flash drive or external hard drive you want to clean.
    选择路径：在出现的文件选择对话框中，选择您要清理的 USB 闪存驱动器或外部硬盘的根目录。
    Start cleaning: After selecting the path, click the "OK" button. The script will start traversing all files and folders under the specified path and delete all Mac-specific files and folders.
    开始清理：选择路径后，点击“确定”按钮。脚本将开始遍历指定路径下的所有文件和文件夹，并删除所有 Mac 特定的文件和文件夹。
    Completion prompt: After the cleaning is completed, a message box will pop up, indicating "Cleaning completed!".
    清理完成提示：清理完成后，会弹出一个消息框，显示“清理完成！”。
