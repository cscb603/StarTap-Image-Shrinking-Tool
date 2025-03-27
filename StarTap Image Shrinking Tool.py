# -*- coding: utf-8 -*-
import os
import sys
import wx
from PIL import Image, ImageEnhance
import threading

# 配置参数
MAX_DIMENSION = 2048
MAX_SIZE_KB = 900
SUFFIX = "_xiao"
QUALITY_STEP = 5
SHARPNESS = 1.2

class ImageProcessor:
    @staticmethod
    def shrink_image(input_path):
        try:
            with Image.open(input_path) as img:
                img = img.convert("RGB")
                scale = MAX_DIMENSION / max(img.size)
                if scale >= 1:
                    return input_path
                
                new_size = tuple(round(d * scale) for d in img.size)
                resized = img.resize(new_size, Image.LANCZOS)
                sharpened = ImageEnhance.Sharpness(resized).enhance(SHARPNESS)
                
                base = os.path.basename(input_path)
                name, ext = os.path.splitext(base)
                output_path = os.path.join(
                    os.path.dirname(input_path),
                    f"{name}{SUFFIX}.jpg"
                )
                
                quality = 90
                while quality >= 10:
                    sharpened.save(output_path, quality=quality, optimize=True)
                    file_size = os.path.getsize(output_path) // 1024
                    if file_size <= MAX_SIZE_KB:
                        return output_path
                    quality -= QUALITY_STEP
                
                os.remove(output_path)
                return None

        except Exception as e:
            wx.MessageBox(f"处理失败：{str(e)}", "错误", wx.OK|wx.ICON_ERROR)
            return None

class MainFrame(wx.Frame):
    def __init__(self):
        super().__init__(None, title="星TAp智能缩图", size=(400, 300))
        panel = wx.Panel(self)
        
        # 界面布局
        vbox = wx.BoxSizer(wx.VERTICAL)
        
        self.label = wx.StaticText(panel, label="拖入图片或点击选择文件")
        self.gauge = wx.Gauge(panel, range=100)
        self.btn = wx.Button(panel, label="选择文件")
        
        vbox.Add(self.label, 0, wx.ALL|wx.ALIGN_CENTER, 20)
        vbox.Add(self.gauge, 0, wx.EXPAND|wx.ALL, 10)
        vbox.Add(self.btn, 0, wx.ALL|wx.ALIGN_CENTER, 20)
        
        panel.SetSizer(vbox)
        
        self.btn.Bind(wx.EVT_BUTTON, self.on_select_files)
        self.DropTarget = FileDropTarget(self)

    def on_select_files(self, event):
        dialog = wx.FileDialog(
            self, 
            "选择图片", 
            wildcard="图片文件|*.jpg;*.jpeg;*.png",
            style=wx.FD_OPEN|wx.FD_MULTIPLE
        )
        if dialog.ShowModal() == wx.ID_OK:
            self.process_files(dialog.GetPaths())

    def process_files(self, paths):
        total = len(paths)
        success = 0
        for i, path in enumerate(paths):
            result = ImageProcessor.shrink_image(path)
            self.gauge.SetValue(int((i+1)/total*100))
            if result:
                success +=1
        wx.MessageBox(f"完成！成功：{success}，失败：{len(paths)-success}", "完成", wx.OK)

class FileDropTarget(wx.FileDropTarget):
    def __init__(self, frame):
        super().__init__()
        self.frame = frame

    def OnDropFiles(self, x, y, filenames):
        self.frame.process_files(filenames)
        return True

if __name__ == "__main__":
    app = wx.App()
    frame = MainFrame()
    frame.Show()
    app.MainLoop()
