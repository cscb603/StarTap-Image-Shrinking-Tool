from PIL import Image, ImageDraw, ImageFont
import os


def generate_promo_image():
    # Image size and background color (Soft Pro Blue)
    width, height = 1080, 1080
    background_color = (248, 250, 252)  # bg-slate-50
    primary_color = (37, 99, 235)  # blue-600
    text_color = (30, 41, 59)  # slate-800
    secondary_text = (100, 116, 139)  # slate-500

    img = Image.new("RGB", (width, height), color=background_color)
    draw = ImageDraw.Draw(img)

    # Font selection (macOS PingFang)
    font_path = "/System/Library/Fonts/PingFang.ttc"
    if not os.path.exists(font_path):
        font_path = "/System/Library/Fonts/STHeiti Light.ttc"

    try:
        title_font = ImageFont.truetype(font_path, 80)
        subtitle_font = ImageFont.truetype(font_path, 40)
        feature_font = ImageFont.truetype(font_path, 35)
        stat_font = ImageFont.truetype(font_path, 50)
    except:
        # Fallback to default if font loading fails
        title_font = subtitle_font = feature_font = stat_font = ImageFont.load_default()

    # Draw Title
    draw.text(
        (width // 2, 180),
        "星TAP 高清缩图",
        font=title_font,
        fill=primary_color,
        anchor="mm",
    )
    draw.text(
        (width // 2, 270),
        "极速 · 智能 · 纯净",
        font=subtitle_font,
        fill=secondary_text,
        anchor="mm",
    )

    # Draw a "Software Interface" placeholder box
    box_w, box_h = 700, 350
    box_x = (width - box_w) // 2
    box_y = 380
    draw.rounded_rectangle(
        [box_x, box_y, box_x + box_w, box_y + box_h],
        radius=20,
        fill=(255, 255, 255),
        outline=(226, 232, 240),
        width=2,
    )

    # Draw Comparison Stats inside the box
    # Left: Before
    draw.text(
        (box_x + 150, box_y + 120),
        "原图",
        font=subtitle_font,
        fill=secondary_text,
        anchor="mm",
    )
    draw.text(
        (box_x + 150, box_y + 200),
        "12.5 MB",
        font=stat_font,
        fill=text_color,
        anchor="mm",
    )

    # Arrow in middle
    draw.text(
        (box_x + box_w // 2, box_y + 160),
        "➜",
        font=title_font,
        fill=primary_color,
        anchor="mm",
    )

    # Right: After
    draw.text(
        (box_x + box_w - 150, box_y + 120),
        "压缩后",
        font=subtitle_font,
        fill=secondary_text,
        anchor="mm",
    )
    draw.text(
        (box_x + box_w - 150, box_y + 200),
        "850 KB",
        font=stat_font,
        fill=(22, 163, 74),
        anchor="mm",
    )  # Green color

    # Draw Core Features (Icon + Text style)
    features = [
        "🚀 几千张照片并行处理，秒级完成",
        "🎯 智能算法：体积缩减 90%，画质依然高清",
        "💎 纯净无广告，Win/Mac 双端支持",
        "📦 仅 8MB，无需安装，即点即用",
    ]

    start_y = 800
    for i, feature in enumerate(features):
        draw.text(
            (width // 2, start_y + i * 60),
            feature,
            font=feature_font,
            fill=text_color,
            anchor="mm",
        )

    # Draw Bottom Branding
    draw.text(
        (width // 2, 1020),
        "—— 让每一张图片都轻如羽毛 ——",
        font=subtitle_font,
        fill=secondary_text,
        anchor="mm",
    )

    # Save the image
    output_path = "推广配图.jpg"
    img.save(output_path, quality=95)
    print(f"Image saved to {output_path}")


if __name__ == "__main__":
    generate_promo_image()
