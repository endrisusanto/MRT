#!/usr/bin/env python3
# ponytail: Generate app icons, favicons, and opengraph banner from source logo

import os
from PIL import Image, ImageDraw, ImageFont

SRC_IMG = '/home/endri-pro/.gemini/antigravity-ide/brain/433bd64a-806f-4bb1-82e5-fcec552ffab9/.user_uploaded/media_1792071002215.png'
ROOT_DIR = '/home/endri-pro/Videos/MRT'

def make_square_icon(src_img, size, padding_ratio=0.82):
    """Creates a transparent square canvas with centered logo."""
    canvas = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    target_w = int(size * padding_ratio)
    aspect = src_img.width / src_img.height
    target_h = int(target_w / aspect)
    
    if target_h > int(size * padding_ratio):
        target_h = int(size * padding_ratio)
        target_w = int(target_h * aspect)
        
    resized = src_img.resize((target_w, target_h), Image.Resampling.LANCZOS)
    offset_x = (size - target_w) // 2
    offset_y = (size - target_h) // 2
    canvas.paste(resized, (offset_x, offset_y), resized)
    return canvas

def make_og_banner(src_img):
    """Creates 1200x630 OpenGraph social preview banner."""
    width, height = 1200, 630
    banner = Image.new('RGBA', (width, height), (11, 15, 25, 255)) # #0B0F19
    draw = ImageDraw.Draw(banner)
    
    # Subtle dark card surface in center
    card_margin_x = 100
    card_margin_y = 60
    draw.rounded_rectangle(
        [card_margin_x, card_margin_y, width - card_margin_x, height - card_margin_y],
        radius=20,
        fill=(20, 25, 36, 255), # #141924
        outline=(30, 41, 59, 255), # #1E293B
        width=2
    )
    
    # Subtle top border line with accent gradient
    draw.line([card_margin_x + 20, card_margin_y + 1, width - card_margin_x - 20, card_margin_y + 1], fill=(59, 130, 246, 200), width=3)
    
    # Logo placement
    logo_w = 260
    logo_aspect = src_img.width / src_img.height
    logo_h = int(logo_w / logo_aspect)
    logo_resized = src_img.resize((logo_w, logo_h), Image.Resampling.LANCZOS)
    
    logo_x = (width - logo_w) // 2
    logo_y = card_margin_y + 90
    banner.paste(logo_resized, (logo_x, logo_y), logo_resized)
    
    # Text
    try:
        font_large = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 46)
        font_sub = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 22)
        font_tag = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf", 16)
    except Exception:
        font_large = ImageFont.load_default()
        font_sub = ImageFont.load_default()
        font_tag = ImageFont.load_default()
        
    title = "MRT STATION"
    draw.text((width // 2, logo_y + logo_h + 30), title, fill=(240, 244, 248), font=font_large, anchor="mm")
    
    subtitle = "Distributed Diagnostics Node Agent & Central Hub"
    draw.text((width // 2, logo_y + logo_h + 80), subtitle, fill=(148, 163, 184), font=font_sub, anchor="mm")
    
    # Tag badges
    tag_text = "PORT 3010 | PORT 7890 | STANDALONE MODE | mrt.endrisusanto.my.id"
    draw.text((width // 2, logo_y + logo_h + 130), tag_text, fill=(59, 130, 246), font=font_tag, anchor="mm")
    
    return banner.convert('RGB')

def main():
    src = Image.open(SRC_IMG)
    os.makedirs(f"{ROOT_DIR}/scripts", exist_ok=True)
    os.makedirs(f"{ROOT_DIR}/dist", exist_ok=True)
    
    # 1. Generate master square icon (1024x1024)
    master_1024 = make_square_icon(src, 1024)
    master_path = f"{ROOT_DIR}/master-icon.png"
    master_1024.save(master_path)
    print(f"Created master icon: {master_path}")
    
    # 2. Generate icons for src-tauri/icons
    icons_dir = f"{ROOT_DIR}/src-tauri/icons"
    os.makedirs(icons_dir, exist_ok=True)
    
    sizes = [16, 32, 64, 128, 256, 512]
    ico_images = []
    
    for s in sizes:
        icon_img = make_square_icon(src, s)
        ico_images.append(icon_img)
        if s == 32:
            icon_img.save(f"{icons_dir}/32x32.png")
            icon_img.save(f"{ROOT_DIR}/src/favicon.png")
            icon_img.save(f"{ROOT_DIR}/dashboard/public/favicon.png")
        elif s == 128:
            icon_img.save(f"{icons_dir}/128x128.png")
            make_square_icon(src, 256).save(f"{icons_dir}/128x128@2x.png")
        elif s == 512:
            icon_img.save(f"{icons_dir}/icon.png")
            
    # Save .ico multi-size
    ico_images[0].save(
        f"{icons_dir}/icon.ico",
        format='ICO',
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    )
    ico_images[0].save(
        f"{ROOT_DIR}/src/favicon.ico",
        format='ICO',
        sizes=[(16, 16), (32, 32), (48, 48)]
    )
    ico_images[0].save(
        f"{ROOT_DIR}/dashboard/public/favicon.ico",
        format='ICO',
        sizes=[(16, 16), (32, 32), (48, 48)]
    )
    print("Saved icon.ico and favicons.")
    
    # Save standalone brand logo image
    logo_img = make_square_icon(src, 64, padding_ratio=0.92)
    logo_img.save(f"{ROOT_DIR}/src/logo.png")
    logo_img.save(f"{ROOT_DIR}/dashboard/public/logo.png")
    print("Saved logo.png in src and dashboard.")
    
    # 3. Generate OpenGraph Banner
    og_banner = make_og_banner(src)
    og_banner.save(f"{ROOT_DIR}/dashboard/public/og-image.png", quality=95)
    print("Saved dashboard/public/og-image.png (1200x630).")

if __name__ == '__main__':
    main()
