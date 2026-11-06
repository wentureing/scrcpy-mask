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

const BINDING_I18N_KEYS: Record<string, string> = {
  "M-Left": "bindings.mouseLeft",
  "M-Middle": "bindings.mouseMiddle",
  "M-Right": "bindings.mouseRight",
  "M-Back": "bindings.mouseBack",
  "M-Forward": "bindings.mouseForward",
  ScrollUp: "bindings.scrollUp",
  ScrollDown: "bindings.scrollDown",
  MouseMoveLeft: "bindings.mouseMoveLeft",
  MouseMoveRight: "bindings.mouseMoveRight",
};

/** 无 i18n 上下文时的中文兜底（按钮圆圈内、编辑器里直接显示用） */
const BINDING_ZH_FALLBACK: Record<string, string> = {
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

/** 映射类型的中文名（编辑器/按钮上直接显示，避免出现 SingleTap 这类英文） */
const MAPPING_TYPE_ZH: Record<string, string> = {
  SingleTap: "单击",
  RepeatTap: "按住连点",
  MultipleTap: "多点序列",
  Swipe: "划动",
  DirectionPad: "方向轮盘",
  PadCastSpell: "摇杆施法",
  MouseCastSpell: "鼠标指向施法",
  CancelCast: "取消施法",
  Observation: "自由视角",
  Fps: "FPS 视角锁定",
  Fire: "开火",
  RawInput: "鼠标直控",
  Script: "脚本",
};

/** 映射类型 → 中文显示名 */
export function formatMappingType(type: string): string {
  return MAPPING_TYPE_ZH[type] ?? type;
}

/** 手柄轴的中文名（圈内显示用，避免出现 LeftStickX 这类英文） */
const AXIS_ZH: Record<string, string> = {
  LeftStickX: "左摇杆X",
  LeftStickY: "左摇杆Y",
  RightStickX: "右摇杆X",
  RightStickY: "右摇杆Y",
  LeftZ: "左扳机",
  RightZ: "右扳机",
};

/** 手柄轴 → 中文显示名（未知轴原样返回） */
export function formatAxisName(name: string): string {
  return AXIS_ZH[name] ?? name;
}

/**
 * 把绑定值（如 "M-Left"、"KeyW"）转成当前语言的显示名。
 * - 中文环境显示“鼠标左键 / W 键”等；
 * - 其它语言显示各自文案；缺键时回退到 en-US，再缺则回退原始值；
 * - 存储值始终不变，避免影响已有映射文件。
 */
export function formatBindingName(
  name: string,
  t?: (key: string, options?: Record<string, unknown>) => string,
): string {
  const translate = (key: string, options?: Record<string, unknown>, fallback?: string) => {
    const zhFallback = fallback ?? BINDING_ZH_FALLBACK[name] ?? name;
    if (!t) return zhFallback;
    const value = t(key, options);
    // i18next 缺键时返回 key 本身
    return value && value !== key ? value : zhFallback;
  };

  const fixedKey = BINDING_I18N_KEYS[name];
  if (fixedKey) return translate(fixedKey);

  if (name.startsWith("M-Other-")) {
    const n = name.slice("M-Other-".length);
    return translate("bindings.mouseOther", { n }, `鼠标键 ${n}`);
  }
  if (name.startsWith("Key")) {
    const k = name.slice(3);
    return translate("bindings.key", { key: k }, `${k} 键`);
  }
  if (name.startsWith("Digit")) {
    const k = name.slice(5);
    return translate("bindings.key", { key: k }, `${k} 键`);
  }
  if (name.startsWith("G-")) {
    const g = name.slice(2);
    return translate("bindings.gamepad", { name: g }, `手柄 ${g}`);
  }
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
