# StarTap Image Shrinking Tool（WeChat）

中文介绍

StarTap Image Shrinking Tool 是专为微信、朋友圈及网络发图打造的宝藏级缩图工具！精准攻克图片在微信发送、朋友圈发布时被二次压缩的难题，让图片 “体积小” 与 “清晰度高” 兼得，真正实现 “小而美”。
支持 JPEG、PNG 等多种格式，拖拽或点击即可轻松添加图片，自动生成适配网络传播的缩图，搭配进度条实时反馈处理进度，结果清晰明了。更提供苹果版与 Windows 版文件，无论使用何种设备，都能让你在网络发图时告别二次压缩，轻松分享清晰美照！

英文介绍

StarTap Image Shrinking Tool—a treasure - level image - shrinking utility tailored for sharing images on WeChat, Moments, and other online platforms! It effectively solves the issue of images being re - compressed when sent via WeChat or posted on Moments, ensuring images stay "small in size" while retaining "high definition"—truly achieving "small and beautiful". Supporting formats like JPEG and PNG, you can add images effortlessly via drag - and - drop or clicks. The tool automatically generates resized images for online sharing, with a progress bar for real - time feedback. We also offer Apple and Windows versions. No matter which device you use, say goodbye to re - compression worries and share clear, beautiful photos with ease!

程序具体功能说明

1. 精准尺寸适配：将图片最大尺寸缩至 2048 像素以内，适配微信等平台展示规则，从尺寸层面避免二次压缩。
2. 智能体积优化：全力将图片文件大小控制在 900KB 以内，通过渐进式调整图片质量，在保障清晰度的同时，让体积符合网络传播标准，杜绝因过大被平台压缩。
3. 锐度强化处理：缩放过程中启用锐度增强（锐度系数 1.2），确保图片缩小后依然清晰锐利，发至微信、朋友圈等场景，细节依旧精致。
4. 批量高效处理：支持一次处理多个图片文件，同步显示进度，快速完成多图优化，满足批量发图需求。
5. 极简操作体验：直观图形界面，拖放图片或点击按钮即可添加文件，零门槛操作，轻松上手。
   
需要的库

os：Python 标准库，用于文件和目录操作。
sys：Python 标准库，提供对 Python 解释器的访问与操作。
wx：wxPython 库，用于创建图形用户界面（GUI）。
PIL（Pillow）：用于图像打开、处理与保存，其中 Image 模块处理基本图像操作，ImageEnhance 模块实现图像增强。
threading：Python 标准库（代码中导入未实际使用，可用于潜在多线程优化）。

运行源代码前，需安装 wxPython 和 Pillow 库，命令如下：
bash
pip install wxPython pillow

如果是下载编译好的程序，那么
【Windows 系统使用步骤】

1 解压工具：找到压缩包，把里面的 “星TAP 缩图工具 1.0.exe” 解压到电脑（放桌面这种好找的地方就行）。
2 打开工具：双击 “星TAP 缩图工具 1.0.exe” 程序。
3 拖图压缩：把要压缩的图片直接拖进程序窗口，结束后去原图片的文件夹，找文件名带 “_xiao” 的图，发圈传图就用它！


【Mac 系统使用步骤】

安装工具（两种方法选其一）：

1 方法一：打开压缩包星 Tap 缩图工具 1.0_Mac.app.zip，直接把 “星Tap 缩图工具 1.0_Mac.app” 拖到桌面，以后双击桌面图标就能用。
2 方法二：打开压缩包，把 “星Tap 缩图工具 1.0_Mac.app” 拖进左下角 “访达” 里的 “应用程序” 文件夹。之后在启动台找到它，双击打开。
3 压缩图片：打开工具后，把图片拖进窗口，处理完去原图片文件夹，找文件名带 “_xiao” 的图，发朋友圈、微信用它超方便！

反馈方式
使用中有问题或建议，欢迎反馈到邮箱：cscb603@qq.com，祝用得顺手～

【程序说明】：
缩图大小为了适配朋友圈和网路分享，又小又清楚。

因为在微信群发图和票圈，长边大于 2048像素，大小大于900kb，微信官方将启动二次压缩，变模糊

2048与900kb是阀值，如果压到900kb画质会下降，则智能缩小图片尺寸。

如果缩小图小于1080px依然不行，则保留2048像素最高质量，不再限制900kb大小。
（以上皆是智能判断。）
