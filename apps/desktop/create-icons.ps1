# Simple script to create placeholder icon files for Tauri
# Creates basic 32x32 and 128x128 PNG files, then converts to ICO

Add-Type -AssemblyName System.Drawing

# Function to create a simple colored square icon
function Create-Icon {
    param($size, $path, $color = "DarkBlue")
    
    $bitmap = New-Object System.Drawing.Bitmap($size, $size)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    
    # Fill with background color
    $brush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::$color)
    $graphics.FillRectangle($brush, 0, 0, $size, $size)
    
    # Add a simple "N" for Nodalync
    $font = New-Object System.Drawing.Font("Arial", ($size / 3), [System.Drawing.FontStyle]::Bold)
    $textBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
    $format = New-Object System.Drawing.StringFormat
    $format.Alignment = [System.Drawing.StringAlignment]::Center
    $format.LineAlignment = [System.Drawing.StringAlignment]::Center
    
    $rect = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
    $graphics.DrawString("N", $font, $textBrush, $rect, $format)
    
    # Save as PNG
    $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    
    # Cleanup
    $graphics.Dispose()
    $bitmap.Dispose()
    $brush.Dispose()
    $textBrush.Dispose()
    $font.Dispose()
}

# Create icons
Create-Icon 32 "icons/32x32.png"
Create-Icon 128 "icons/128x128.png"

# Copy 128x128 as the 2x version
Copy-Item "icons/128x128.png" "icons/128x128@2x.png"

Write-Host "Created placeholder icon files successfully!"