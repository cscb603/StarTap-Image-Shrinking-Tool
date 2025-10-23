#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
星TAP高清缩图优化版 v2.6
优化功能：LANCZOS缩放 + 预处理降噪 + 智能锐化 + 自适应JPG压缩
优化点：并行处理提速 + 内存效率提升 + 稳定性增强 + 体验优化
"""

import sys
import os
import math
import logging
import tempfile
import threading
import numpy as np
from functools import lru_cache
from concurrent.futures import ThreadPoolExecutor, as_completed

# 确保在 Windows 系统上运行时隐藏命令行窗口
if os.name == 'nt':
    try:
        import ctypes
        ctypes.windll.user32.ShowWindow(ctypes.windll.kernel32.GetConsoleWindow(), 0)
    except:
        pass

# 检查并导入必要的库
required_libraries = {
    'wx': 'wxPython',
    'PIL': 'Pillow',
    'piexif': 'piexif',
    'numpy': 'numpy'
}

missing_libraries = []
for lib, package in required_libraries.items():
    try:
        __import__(lib)
    except ImportError:
        missing_libraries.append(package)

if missing_libraries:
    print("检测到缺失的依赖库：")
    for lib in missing_libraries:
        print(f"  - {lib}")
    print("\n请运行以下命令安装：")
    print(f"pip install {' '.join(missing_libraries)}")
    sys.exit(1)

# 导入所有库
import wx
from PIL import Image, ImageFilter
import piexif

# 配置日志
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(message)s',
    handlers=[logging.StreamHandler()]
)

class ImageOptimizer:
    # 常量定义 - 集中管理配置参数
    MAX_DIMENSION = 2048  # 普通模式最大尺寸
    MIN_QUALITY = 70      # 最小质量阈值
    TARGET_KB = 900       # 目标文件大小
    HD_QUALITY = 100      # 高清模式质量
    MAX_WORKERS = 4       # 最大并行线程数

    @staticmethod
    @lru_cache(maxsize=100)
    def load_image(path):
        """安全加载图像并保留原始EXIF，添加文件修改时间校验"""
        try:
            mtime = os.path.getmtime(path)  # 获取文件修改时间
            img = Image.open(path)
            exif = img.info.get('exif', b'')
            return img.convert("RGB"), exif, mtime
        except Exception as e:
            logging.error(f"加载失败: {path} - {str(e)}")
            return None, b'', 0

    @staticmethod
    def analyze_image_content(img):
        """使用numpy加速图像内容分析，减少重复计算"""
        try:
            # 转为numpy数组一次性处理（比PIL遍历快10倍以上）
            gray_np = np.array(img.convert('L'))
            total_pixels = gray_np.size

            # 计算直方图和熵值（复杂度）
            histogram, _ = np.histogram(gray_np, bins=256, range=(0, 255))
            entropy = 0.0
            for count in histogram:
                if count > 0:
                    prob = count / total_pixels
                    entropy -= prob * math.log2(prob)

            # 计算对比度
            max_pixel = gray_np.max()
            min_pixel = gray_np.min()
            contrast = (max_pixel - min_pixel) / 255 if max_pixel > min_pixel else 0

            # 内容类型判断
            is_graphic = entropy < 6.5 and contrast > 0.4
            is_portrait = 6.5 <= entropy <= 7.5 and 0.2 <= contrast <= 0.6
            is_landscape = entropy > 7.0 and contrast < 0.4

            return {
                'entropy': entropy,
                'contrast': contrast,
                'is_graphic': is_graphic,
                'is_portrait': is_portrait,
                'is_landscape': is_landscape
            }
        except Exception as e:
            logging.warning(f"图像分析失败: {str(e)}")
            return {
                'entropy': 7.0,
                'contrast': 0.3,
                'is_graphic': False,
                'is_portrait': False,
                'is_landscape': False
            }

    @staticmethod
    def apply_intelligent_denoising(img, content_features):
        """智能降噪预处理"""
        try:
            if content_features['is_graphic']:
                return img.filter(ImageFilter.MedianFilter(size=1))
            elif content_features['is_portrait']:
                return img.filter(ImageFilter.MedianFilter(size=2))
            elif content_features['is_landscape']:
                return img.filter(ImageFilter.GaussianBlur(radius=0.5))
            else:
                return img.filter(ImageFilter.MedianFilter(size=2) 
                                 if content_features['entropy'] > 7.2 
                                 else ImageFilter.MedianFilter(size=1))
        except Exception as e:
            logging.warning(f"降噪处理失败: {str(e)}")
            return img

    @staticmethod
    def apply_intelligent_sharpening(img, content_features, scale=1.0):
        """智能补偿锐化，根据缩放比例动态调整半径"""
        try:
            radius = 1.0 * scale  # 按缩放比例调整锐化半径
            if content_features['is_graphic']:
                return img.filter(ImageFilter.UnsharpMask(
                    radius=min(2.0, radius*2), percent=150, threshold=2))
            elif content_features['is_portrait']:
                return img.filter(ImageFilter.UnsharpMask(
                    radius=min(1.2, radius*1.5), percent=120, threshold=3))
            elif content_features['is_landscape']:
                return img.filter(ImageFilter.UnsharpMask(
                    radius=min(1.0, radius), percent=100, threshold=5))
            else:
                sharpness = 1.1 + (7.5 - content_features['entropy']) * 0.1
                sharpness = max(1.0, min(1.5, sharpness))
                return img.filter(ImageFilter.UnsharpMask(
                    radius=min(1.5, radius), 
                    percent=int(100 * sharpness), 
                    threshold=3
                ))
        except Exception as e:
            logging.warning(f"锐化处理失败: {str(e)}")
            return img

    @staticmethod
    def smart_resize(img, max_dim=MAX_DIMENSION):
        """智能缩放，返回缩放后的图像和缩放比例"""
        try:
            width, height = img.size
            current_max = max(width, height)
            
            if current_max <= max_dim:
                return img, (width, height), 1.0  # 未缩放时比例为1
            
            scale = max_dim / current_max
            new_width = int(width * scale)
            new_height = int(height * scale)
            resized_img = img.resize((new_width, new_height), Image.Resampling.LANCZOS)
            return resized_img, (new_width, new_height), scale
        except Exception as e:
            logging.error(f"缩放失败: {str(e)}")
            return img, img.size, 1.0

    @staticmethod
    def get_best_quality(img, exif, target_kb=TARGET_KB, content_features=None):
        """优化二分法寻找最佳质量参数，减少IO操作"""
        if content_features is None:
            content_features = ImageOptimizer.analyze_image_content(img)
        
        # 动态调整质量范围和步长
        if content_features['is_graphic']:
            low, high = 85, 95
        elif content_features['is_portrait']:
            low, high = 80, 92
        elif content_features['is_landscape']:
            low, high = 75, 88
        else:
            low, high = (75, 88) if content_features['entropy'] > 7.2 else (80, 90)
        
        best_quality = low

        # 使用临时文件管理器自动处理文件生命周期
        with tempfile.NamedTemporaryFile(suffix='.jpg', delete=False) as temp_file:
            temp_path = temp_file.name

        try:
            # 二分查找（质量差≤3时停止，减少尝试次数）
            while low <= high and (high - low) > 3:
                mid = (low + high) // 2
                subsampling = 0 if mid >= 90 else 1
                
                img.save(temp_path, quality=mid, exif=exif, 
                        optimize=True, subsampling=subsampling)
                
                size = os.path.getsize(temp_path) / 1024
                if size <= target_kb:
                    best_quality = mid
                    low = mid + 1
                else:
                    high = mid - 1
            
            best_quality = max(best_quality, ImageOptimizer.MIN_QUALITY)
        except Exception as e:
            logging.error(f"质量优化失败: {str(e)}")
        finally:
            if os.path.exists(temp_path):
                os.remove(temp_path)
        
        return best_quality

    @staticmethod
    def process_single_image(input_path, is_hd_mode):
        """拆分后的单文件处理函数，便于并行调用"""
        try:
            # 加载图片并检查文件是否更新
            cached_img, cached_exif, cached_mtime = ImageOptimizer.load_image(input_path)
            current_mtime = os.path.getmtime(input_path) if os.path.exists(input_path) else 0
            
            # 如果文件已更新，重新加载
            if cached_mtime != current_mtime or cached_img is None:
                try:
                    img = Image.open(input_path).convert("RGB")
                    exif = img.info.get('exif', b'')
                except Exception as e:
                    logging.error(f"重新加载失败: {input_path} - {str(e)}")
                    return False, input_path, ""
            else:
                img, exif = cached_img, cached_exif

            original_width, original_height = img.size
            original_size = os.path.getsize(input_path) / 1024
            logging.info(f"开始处理: {input_path} ({original_width}x{original_height}, {original_size:.1f}KB)")
            
            # 图像内容分析
            content_features = ImageOptimizer.analyze_image_content(img)
            
            # 智能缩放
            scale = 1.0
            if not is_hd_mode:
                img, (new_width, new_height), scale = ImageOptimizer.smart_resize(img)
                logging.info(f"缩放至: {new_width}x{new_height}")
            
            # 智能降噪（非高清模式）
            if not is_hd_mode:
                img = ImageOptimizer.apply_intelligent_denoising(img, content_features)
            
            # 智能锐化
            if is_hd_mode:
                img = img.filter(ImageFilter.UnsharpMask(radius=1.8, percent=130, threshold=2))
            else:
                img = ImageOptimizer.apply_intelligent_sharpening(img, content_features, scale)
            
            # 构建输出路径
            base, ext = os.path.splitext(input_path)
            suffix = "_hdxiao" if is_hd_mode else "_xiao"
            output_path = f"{base}{suffix}.jpg"
            
            # 处理EXIF（容错处理）
            try:
                if exif:
                    exif_dict = piexif.load(exif)
                    exif = piexif.dump(exif_dict)  # 修复可能损坏的EXIF
            except Exception as e:
                logging.warning(f"EXIF修复失败，将忽略: {e}")
                exif = b''
            
            # 质量设置
            if is_hd_mode:
                quality = ImageOptimizer.HD_QUALITY
                subsampling = 0
            else:
                quality = ImageOptimizer.get_best_quality(img, exif, ImageOptimizer.TARGET_KB, content_features)
                subsampling = 0 if quality >= 90 else 1
            
            # 保存图片
            img.save(
                output_path,
                quality=quality,
                optimize=True,
                subsampling=subsampling,
                exif=exif
            )
            
            # 验证输出
            if os.path.exists(output_path):
                output_size = os.path.getsize(output_path) / 1024
                compress_ratio = (1 - output_size / original_size) * 100 if original_size > 0 else 0
                logging.info(f"处理完成: {output_path} ({quality}质量, {output_size:.1f}KB, 压缩{compress_ratio:.1f}%)")
                return True, input_path, ""
            else:
                error = "保存失败"
                logging.error(f"{error}: {output_path}")
                return False, input_path, error
                
        except Exception as e:
            error = str(e)
            logging.error(f"处理失败: {input_path} - {error}", exc_info=True)
            return False, input_path, error

class MainWindow(wx.Frame):
    def __init__(self):
        super().__init__(None, title="星TAP高清缩图优化版 v2.6", size=(480, 380))
        self.init_ui()
        self.SetBackgroundColour(wx.Colour(240, 240, 240))
        self.Bind(wx.EVT_CLOSE, self.on_close)
        
        # 状态变量
        self.is_processing = False
        self.total_files = 0
        self.processed = 0
        self.failed = 0
        self.failed_reasons = []  # 记录失败原因

    def init_ui(self):
        """初始化UI界面"""
        panel = wx.Panel(self)
        vbox = wx.BoxSizer(wx.VERTICAL)
        
        # 标题
        title = wx.StaticText(panel, label="星TAP高清缩图优化版", style=wx.ALIGN_CENTER)
        title_font = wx.FontInfo(16).Family(wx.FONTFAMILY_DEFAULT).Weight(wx.FONTWEIGHT_BOLD)
        title.SetFont(wx.Font(title_font))
        vbox.Add(title, 0, wx.ALL|wx.EXPAND, 10)
        
        # 版本信息
        version = wx.StaticText(panel, label="v2.6 | 并行加速优化", style=wx.ALIGN_CENTER)
        version_font = wx.FontInfo(10).Family(wx.FONTFAMILY_DEFAULT).Style(wx.FONTSTYLE_ITALIC)
        version.SetFont(wx.Font(version_font))
        version.SetForegroundColour(wx.Colour(100, 100, 100))
        vbox.Add(version, 0, wx.ALL|wx.EXPAND, 5)
        
        # 模式选择
        self.mode_radio = wx.RadioBox(
            panel, 
            label="输出模式",
            choices=["微信优化模式(≈900KB)", "无损缩图模式(高质量)"],
            majorDimension=1,
            style=wx.RA_SPECIFY_ROWS
        )
        vbox.Add(self.mode_radio, 0, wx.ALL|wx.EXPAND, 10)
        
        # 优化特性说明
        features = [
            "• LANCZOS高质量缩放",
            "• 智能降噪预处理", 
            "• 内容感知锐化（自适应半径）",
            "• 自适应JPG压缩",
            "• 多线程并行加速"
        ]
        features_text = "\n".join(features)
        features_label = wx.StaticText(panel, label=features_text)
        features_font = wx.FontInfo(9).Family(wx.FONTFAMILY_DEFAULT)
        features_label.SetFont(wx.Font(features_font))
        features_label.SetForegroundColour(wx.Colour(80, 80, 80))
        vbox.Add(features_label, 0, wx.LEFT|wx.RIGHT|wx.BOTTOM, 10)
        
        # 拖放区域
        drop_panel = wx.Panel(panel, style=wx.SUNKEN_BORDER)
        drop_panel.SetBackgroundColour(wx.Colour(255, 255, 255))
        drop_text = wx.StaticText(drop_panel, label="拖放图片到此处\n或点击选择图片", style=wx.ALIGN_CENTER)
        drop_font = wx.FontInfo(12).Family(wx.FONTFAMILY_DEFAULT)
        drop_text.SetFont(wx.Font(drop_font))
        drop_panel.Bind(wx.EVT_LEFT_DOWN, self.on_select_files)
        drop_panel.SetDropTarget(FileDropTarget(self))
        
        drop_sizer = wx.BoxSizer(wx.VERTICAL)
        drop_sizer.Add(drop_text, 1, wx.ALL|wx.EXPAND, 30)
        drop_panel.SetSizer(drop_sizer)
        vbox.Add(drop_panel, 1, wx.ALL|wx.EXPAND, 10)
        
        # 进度区域
        progress_sizer = wx.BoxSizer(wx.HORIZONTAL)
        
        self.progress = wx.Gauge(panel, range=100)
        progress_sizer.Add(self.progress, 1, wx.EXPAND)
        
        self.status_text = wx.StaticText(panel, label="准备就绪")
        progress_sizer.Add(self.status_text, 0, wx.LEFT|wx.RIGHT, 10)
        
        vbox.Add(progress_sizer, 0, wx.ALL|wx.EXPAND, 10)
        
        panel.SetSizer(vbox)

    def on_select_files(self, event):
        """文件选择对话框"""
        if self.is_processing:
            wx.MessageBox("处理正在进行中，请稍候...", "提示", wx.OK|wx.ICON_INFORMATION)
            return
            
        with wx.FileDialog(
            self, "选择图片文件", 
            wildcard="图片文件|*.jpg;*.jpeg;*.png|所有文件|*.*",
            style=wx.FD_OPEN|wx.FD_MULTIPLE
        ) as dlg:
            if dlg.ShowModal() == wx.ID_OK:
                self.start_processing(dlg.GetPaths())

    def start_processing(self, paths):
        """开始处理（初始化状态）"""
        if not paths or self.is_processing:
            return
            
        # 过滤有效的图片文件（增加权限检查）
        valid_paths = []
        for path in paths:
            ext = os.path.splitext(path)[1].lower()
            if ext in ['.jpg', '.jpeg', '.png']:
                if os.access(path, os.R_OK):  # 检查读权限
                    valid_paths.append(path)
                else:
                    logging.warning(f"无权限访问: {path}")
                    self.failed_reasons.append(f"{os.path.basename(path)}: 无读取权限")
        
        if not valid_paths:
            wx.MessageBox("未选择有效的图片文件！", "错误", wx.OK|wx.ICON_ERROR)
            return
            
        self.is_processing = True
        self.total_files = len(valid_paths)
        self.processed = 0
        self.failed = 0
        self.failed_reasons = []  # 重置失败原因
        self.progress.SetValue(0)
        
        # 启动并行处理线程
        thread = threading.Thread(
            target=self.process_files,
            args=(valid_paths,),
            daemon=True
        )
        thread.start()

    def process_files(self, paths):
        """并行处理文件（多线程加速）"""
        is_hd_mode = self.mode_radio.GetSelection() == 1
        max_workers = min(ImageOptimizer.MAX_WORKERS, os.cpu_count() or 2)
        
        with ThreadPoolExecutor(max_workers=max_workers) as executor:
            # 提交所有任务
            futures = {
                executor.submit(ImageOptimizer.process_single_image, path, is_hd_mode): path
                for path in paths
            }
            
            # 处理完成的任务
            for i, future in enumerate(as_completed(futures)):
                path = futures[future]
                filename = os.path.basename(path)
                try:
                    success, _, error = future.result()
                    if success:
                        self.processed += 1
                    else:
                        self.failed += 1
                        if error:
                            self.failed_reasons.append(f"{filename}: {error}")
                except Exception as e:
                    self.failed += 1
                    self.failed_reasons.append(f"{filename}: 线程处理异常 - {str(e)}")
                    logging.error(f"线程异常: {path} - {str(e)}")
                
                # 更新进度和状态
                progress = int((i + 1) / self.total_files * 100)
                wx.CallAfter(self.update_progress, progress)
                wx.CallAfter(
                    self.update_status, 
                    f"处理中: {filename} ({i+1}/{self.total_files})"
                )
        
        wx.CallAfter(self.finish_processing)

    def update_progress(self, value):
        """更新进度条"""
        self.progress.SetValue(value)

    def update_status(self, text):
        """更新状态文本"""
        self.status_text.SetLabel(text)

    def finish_processing(self):
        """处理完成回调"""
        self.is_processing = False
        if self.processed + self.failed == 0:
            self.status_text.SetLabel("处理已取消")
        else:
            status = f"完成! 成功 {self.processed}/{self.total_files} "
            if self.failed > 0:
                status += f"(失败 {self.failed})"
            self.status_text.SetLabel(status)
            
            # 构建结果消息（包含失败原因）
            message = f"处理完成！\n\n成功: {self.processed} 个文件\n失败: {self.failed} 个文件"
            if self.failed_reasons:
                message += "\n\n失败原因:\n" + "\n".join(self.failed_reasons[:5])  # 显示前5条
                if len(self.failed_reasons) > 5:
                    message += f"\n... 还有 {len(self.failed_reasons)-5} 条未显示"
            message += "\n\n输出文件已保存到原文件夹"
            wx.MessageBox(message, "处理完成", wx.OK|wx.ICON_INFORMATION)

    def on_close(self, event):
        """关闭窗口事件"""
        if self.is_processing:
            if wx.MessageBox("处理正在进行中，确定要退出吗？", 
                           "警告", 
                           wx.YES_NO|wx.ICON_WARNING) == wx.NO:
                return
        self.Destroy()

class FileDropTarget(wx.FileDropTarget):
    def __init__(self, window):
        super().__init__()
        self.window = window

    def OnDropFiles(self, x, y, filenames):
        """处理拖放文件"""
        self.window.start_processing(filenames)
        return True

def main():
    """主函数"""
    try:
        app = wx.App()
        window = MainWindow()
        window.Show()
        app.MainLoop()
    except Exception as e:
        logging.critical(f"程序启动失败: {str(e)}", exc_info=True)
        wx.MessageBox(f"程序启动失败: {str(e)}", "错误", wx.OK|wx.ICON_ERROR)

if __name__ == "__main__":
    main()
