#!/usr/bin/env python3
# ponytail: Generate app icons, favicons, and opengraph banner from source logo

import os
from PIL import Image, ImageDraw, ImageFont

SRC_IMG = '/home/endri-pro/.gemini/antigravity-ide/brain/433bd64a-806f-4bb1-82e5-fcec552ffab9/.user_uploaded/media_1792071002215.png'
ROOT_DIR = '/home/endri-pro/Videos/MRT'

def make_circle_icon(src_img, size, padding_ratio=0.74, margin_ratio=0.03):
    """Creates a circular white badge canvas with centered logo."""
    scale = 2 if size <= 512 else 1
    canvas_size = size * scale

    canvas = Image.new('RGBA', (canvas_size, canvas_size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(canvas)

    margin = int(canvas_size * margin_ratio)
    circle_box = [margin, margin, canvas_size - margin, canvas_size - margin]
    draw.ellipse(circle_box, fill=(255, 255, 255, 255))

    diam = canvas_size - 2 * margin
    target_w = int(diam * padding_ratio)
    aspect = src_img.width / src_img.height
    target_h = int(target_w / aspect)

    if target_h > int(diam * padding_ratio):
        target_h = int(diam * padding_ratio)
        target_w = int(target_h * aspect)

    resized_logo = src_img.resize((target_w, target_h), Image.Resampling.LANCZOS)
    pos_x = (canvas_size - target_w) // 2
    pos_y = (canvas_size - target_h) // 2
    canvas.paste(resized_logo, (pos_x, pos_y), resized_logo)

    if scale > 1:
        canvas = canvas.resize((size, size), Image.Resampling.LANCZOS)
    return canvas

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
    
    # Logo placement with white circle badge for uniformity
    badge_size = 200
    badge = make_circle_icon(src_img, badge_size, padding_ratio=0.74)
    logo_x = (width - badge_size) // 2
    logo_y = card_margin_y + 60
    banner.paste(badge, (logo_x, logo_y), badge)
    
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
    draw.text((width // 2, logo_y + badge_size + 30), title, fill=(240, 244, 248), font=font_large, anchor="mm")
    
    subtitle = "Distributed Diagnostics Node Agent & Central Hub"
    draw.text((width // 2, logo_y + badge_size + 80), subtitle, fill=(148, 163, 184), font=font_sub, anchor="mm")
    
    # Tag badges
    tag_text = "PORT 3010 | PORT 7890 | STANDALONE MODE | mrt.endrisusanto.my.id"
    draw.text((width // 2, logo_y + badge_size + 130), tag_text, fill=(59, 130, 246), font=font_tag, anchor="mm")
    
    return banner.convert('RGB')

def main():
    import subprocess
    src = Image.open(SRC_IMG)
    os.makedirs(f"{ROOT_DIR}/scripts", exist_ok=True)
    os.makedirs(f"{ROOT_DIR}/dist", exist_ok=True)
    
    # 1. Generate master circle icon (1024x1024)
    master_1024 = make_circle_icon(src, 1024)
    master_path = f"{ROOT_DIR}/master-icon.png"
    master_1024.save(master_path)
    print(f"Created master circle icon: {master_path}")
    
    # 2. Generate full Tauri icon suite (ICO, ICNS, PNGs)
    icons_dir = f"{ROOT_DIR}/src-tauri/icons"
    os.makedirs(icons_dir, exist_ok=True)
    subprocess.run(['npx', 'tauri', 'icon', master_path, '-o', icons_dir], cwd=ROOT_DIR, check=True)
    print(f"Generated Tauri icons in {icons_dir}")
    
    # 3. Generate Favicons (PNG & multi-size ICO)
    fav32 = make_circle_icon(src, 32)
    fav32.save(f"{ROOT_DIR}/src/favicon.png")
    fav32.save(f"{ROOT_DIR}/dashboard/public/favicon.png")
    
    fav_sizes = [make_circle_icon(src, s) for s in [16, 32, 48]]
    for target in [f"{ROOT_DIR}/src/favicon.ico", f"{ROOT_DIR}/dashboard/public/favicon.ico"]:
        fav_sizes[0].save(target, format='ICO', sizes=[(16, 16), (32, 32), (48, 48)])
    print("Saved favicons in src and dashboard/public.")
    
    # 4. Save standalone transparent brand logo image
    logo_img = make_square_icon(src, 64, padding_ratio=0.92)
    logo_img.save(f"{ROOT_DIR}/src/logo.png")
    logo_img.save(f"{ROOT_DIR}/dashboard/public/logo.png")
    print("Saved logo.png in src and dashboard.")
    
    # 5. Generate OpenGraph Banner
    og_banner = make_og_banner(src)
    og_banner.save(f"{ROOT_DIR}/dashboard/public/og-image.png", quality=95)
    print("Saved dashboard/public/og-image.png (1200x630).")

if __name__ == '__main__':
    main()
