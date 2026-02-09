from PIL import Image, ImageDraw
import os


def create_rounded_mask(size, radius):
    mask = Image.new("L", size, 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0) + size, radius=radius, fill=255)
    return mask


def create_high_quality_ico(png_path, ico_path):
    try:
        img = Image.open(png_path).convert("RGBA")

        # Apply rounded corners mask to ensure transparency
        # Using a radius proportional to the size, e.g., 20% of width
        w, h = img.size
        radius = int(min(w, h) * 0.18)  # Adjust radius to match user's screenshot
        mask = create_rounded_mask(img.size, radius)

        # Create a new image with transparency
        rounded_img = Image.new("RGBA", img.size, (0, 0, 0, 0))
        rounded_img.paste(img, (0, 0), mask=mask)

        sizes = [(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (16, 16)]

        imgs = []
        for size in sizes:
            # For each size, we need to re-apply the mask or just resize the already masked image
            # Resizing the already masked image with LANCZOS is better
            resampled = rounded_img.resize(size, Image.Resampling.LANCZOS)
            imgs.append(resampled)

        # Save as ICO
        imgs[0].save(
            ico_path,
            format="ICO",
            sizes=[(i.width, i.height) for i in imgs],
            append_images=imgs[1:],
        )
        print(f"Successfully created rounded high-res ICO at {ico_path}")

    except Exception as e:
        print(f"Error: {e}")


if __name__ == "__main__":
    src = r"F:\trae-cn\rust_image_compressor\高速缩图图标.png"
    dst = r"F:\trae-cn\rust_image_compressor\icon.ico"

    if os.path.exists(src):
        create_high_quality_ico(src, dst)
    else:
        print(f"Source not found: {src}")
