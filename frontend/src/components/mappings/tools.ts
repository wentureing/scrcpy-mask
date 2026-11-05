export function mappingButtonPresetStyle(
  radiusX: number,
  radiusY?: number,
): React.CSSProperties {
  radiusY = radiusY ?? radiusX;

  return {
    width: radiusX,
    height: radiusY,
    left: -radiusX / 2,
    top: -radiusY / 2,
  };
}

export function mappingButtonScaledPresetStyle(
  radiusX: number,
  viewportSize: { width: number; height: number },
  radiusY?: number,
): React.CSSProperties {
  const buttonScale = Math.max(viewportSize.width, viewportSize.height) / 960;
  return mappingButtonPresetStyle(
    Math.round(radiusX * buttonScale),
    Math.round((radiusY ?? radiusX) * buttonScale),
  );
}

export function clientPositionToMappingPosition(
  cX: number,
  cY: number,
  maskArea: { width: number; height: number; left: number; top: number },
  oW: number,
  oH: number,
) {
  const mappingContainer = document.getElementById("mappings-container") as HTMLElement;
  const scrollX = mappingContainer?.scrollLeft ?? 0;
  const scrollY = mappingContainer?.scrollTop ?? 0;
  const mX = Math.max(0, Math.min(maskArea.width, cX + scrollX - maskArea.left));
  const mY = Math.max(0, Math.min(maskArea.height, cY + scrollY - maskArea.top));
  return {
    x: Math.round((mX / maskArea.width) * oW),
    y: Math.round((mY / maskArea.height) * oH),
  };
}

export function mappingButtonPosition(
  oX: number,
  oY: number,
  scale: { x: number; y: number },
) {
  return {
    x: Math.round(oX * scale.x),
    y: Math.round(oY * scale.y),
  };
}

export function mappingButtonTransformStyle(
  oX: number,
  oY: number,
  scale: { x: number; y: number },
): string {
  const x = Math.round(oX * scale.x);
  const y = Math.round(oY * scale.y);
  return `translate(${x}px, ${y}px)`;
}

/**
 * 把绑定值（如 "M-Left"、"KeyW"）转成中文显示名。
 * 存储仍用原始值，只有展示层转中文，避免影响已有的映射配置文件。
 */
export function formatBindingName(name: string): string {
  const fixed: Record<string, string> = {
    "M-Left": "鼠标左键",
    "M-Middle": "鼠标中键",
    "M-Right": "鼠标右键",
    "M-Back": "鼠标侧键·后退",
    "M-Forward": "鼠标侧键·前进",
    ScrollUp: "滚轮向上",
    ScrollDown: "滚轮向下",
    MouseMoveLeft: "鼠标左移",
    MouseMoveRight: "鼠标右移",
  };
  if (fixed[name]) return fixed[name];
  if (name.startsWith("M-Other-")) return `鼠标键 ${name.slice("M-Other-".length)}`;
  if (name.startsWith("Key")) return `${name.slice(3)} 键`;
  if (name.startsWith("Digit")) return `${name.slice(5)} 键`;
  if (name.startsWith("G-")) return `手柄 ${name.slice(2)}`;
  return name;
}

export function mappingButtonDragFactory(
  maskArea: { width: number; height: number; left: number; top: number },
  originalSize: { width: number; height: number },
  onMouseUp: ({ x, y }: { x: number; y: number }) => void,
  delay?: number,
) {
  // delay > 0 时保留"长按才拖"的旧行为；默认 0 表示按下即按位移阈值判定，手感更跟手。
  const longPressDelay = delay ?? 0;
  const DRAG_THRESHOLD = 4;

  const handleDrag = (downEvent: React.MouseEvent) => {
    if (downEvent.button !== 0) return;

    const mappingContainer = document.getElementById("mappings-container") as HTMLElement;
    const scrollX = mappingContainer?.scrollLeft ?? 0;
    const scrollY = mappingContainer?.scrollTop ?? 0;

    const { width, height, left, top } = maskArea;
    const element = downEvent.currentTarget as HTMLElement;

    const startX = downEvent.clientX;
    const startY = downEvent.clientY;
    let dragStarted = false;
    let longPressTimer = 0;
    let rafId = 0;
    let curMaskX = 0;
    let curMaskY = 0;

    const updateCurMaskPos = (e: MouseEvent | React.MouseEvent) => {
      curMaskX = Math.max(0, Math.min(e.clientX + scrollX - left, width));
      curMaskY = Math.max(0, Math.min(e.clientY + scrollY - top, height));
    };

    // 用 rAF 合并同一帧内的多次 mousemove，避免高频写 style 造成掉帧
    const scheduleMove = () => {
      if (rafId) return;
      rafId = requestAnimationFrame(() => {
        rafId = 0;
        element.style.transform = `translate(${curMaskX}px, ${curMaskY}px)`;
      });
    };

    const handleMouseMove = (moveEvent: MouseEvent) => {
      updateCurMaskPos(moveEvent);
      if (!dragStarted && longPressDelay <= 0) {
        const dx = Math.abs(moveEvent.clientX - startX);
        const dy = Math.abs(moveEvent.clientY - startY);
        if (dx < DRAG_THRESHOLD && dy < DRAG_THRESHOLD) return;
        dragStarted = true;
        element.style.cursor = "grabbing";
      }
      if (!dragStarted) return;
      scheduleMove();
    };

    const handleMouseUp = (upEvent: MouseEvent) => {
      clearTimeout(longPressTimer);
      if (rafId) {
        cancelAnimationFrame(rafId);
        rafId = 0;
      }
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
      element.style.cursor = "";
      if (!dragStarted) return;

      updateCurMaskPos(upEvent);
      element.style.transform = `translate(${curMaskX}px, ${curMaskY}px)`;

      onMouseUp({
        x: Math.round((curMaskX / width) * originalSize.width),
        y: Math.round((curMaskY / height) * originalSize.height),
      });
    };

    updateCurMaskPos(downEvent);
    window.addEventListener("mousemove", handleMouseMove);
    if (longPressDelay > 0) {
      longPressTimer = window.setTimeout(() => {
        dragStarted = true;
        element.style.transform = `translate(${curMaskX}px, ${curMaskY}px)`;
      }, longPressDelay);
    }
    window.addEventListener("mouseup", handleMouseUp);
  };

  return handleDrag;
}

export function mappingModalDragFactory(delay?: number) {
  delay = delay ?? 150;
  const handleDrag = (downEvent: React.MouseEvent) => {
    if (downEvent.button !== 0) return;
    const element = document.querySelector(".setting-modal") as HTMLElement;

    const oX = downEvent.pageX;
    const oY = downEvent.pageY;
    let dragStarted = false;
    let longPressTimer = 0;
    let cX = oX;
    let cY = oY;
    const match = element.style.transform.match(
      /translate\((-?\d+)px,\s*(-?\d+)px\)/,
    );
    const oldTX = match ? parseInt(match[1]) : 0;
    const oldTY = match ? parseInt(match[2]) : 0;

    const handleMouseMove = (moveEvent: MouseEvent) => {
      cX = moveEvent.pageX;
      cY = moveEvent.pageY;
      if (!dragStarted) return;
      element.style.transform = `translate(${oldTX + cX - oX}px, ${oldTY + cY - oY}px)`;
    };

    const handleMouseUp = (upEvent: MouseEvent) => {
      clearTimeout(longPressTimer);
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
      if (!dragStarted) return;
      cX = upEvent.pageX;
      cY = upEvent.pageY;
      element.style.transform = `translate(${oldTX + cX - oX}px, ${oldTY + cY - oY}px)`;
    };
    cX = downEvent.pageX;
    cY = downEvent.pageY;
    window.addEventListener("mousemove", handleMouseMove);
    longPressTimer = window.setTimeout(() => {
      dragStarted = true;
      element.style.transform = `translate(${oldTX + cX - oX}px, ${oldTY + cY - oY}px)`;
    }, delay);
    window.addEventListener("mouseup", handleMouseUp);
  };
  return handleDrag;
}
