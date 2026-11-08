import { useState } from "react";
import { Select, Slider, Switch, Typography } from "antd";
import { useTranslation } from "react-i18next";
import { ItemBox, ItemBoxContainer } from "../common/ItemBox";
import { SettingModal } from "./Common";
import { useAppDispatch, useAppSelector } from "../../store/store";
import {
  forceSetLocalConfig,
  setFreeLookEnabled,
  setFreeLookInvertX,
  setFreeLookInvertY,
  setFreeLookSensitivity,
} from "../../store/localConfig";
import { requestPost } from "../../utils";
import { useMessageContext } from "../../hooks";

/**
 * 无点击转视角（free look）在映射画布上的入口圈。
 *
 * 用例 13 要求"对应的映射圈呈现在左半屏中间，方便用户进入编辑"：
 * 这里放一个固定位置的圆形入口，点击打开简版设置（开关/灵敏度/翻转/退出键）。
 */
export default function FreeLookEntry({
  open: externalOpen,
  onClose,
}: {
  /** 由映射页右键菜单控制打开；不传时由入口圈自己管理 */
  open?: boolean;
  onClose?: () => void;
} = {}) {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const messageApi = useMessageContext();
  const localConfig = useAppSelector((state) => state.localConfig);
  const [internalOpen, setInternalOpen] = useState(false);
  const open = internalOpen || !!externalOpen;
  const close = () => {
    setInternalOpen(false);
    onClose?.();
  };

  // 退出键冲突时后端会拒绝保存（D10），所以成功后才回填 store。
  async function changeExitKeys(value: string[]) {
    try {
      const res = await requestPost("/api/config/update_config", {
        key: "free_look_exit_keys",
        value,
      });
      dispatch(forceSetLocalConfig({ free_look_exit_keys: value }));
      messageApi?.success(res.message);
    } catch (err: unknown) {
      messageApi?.error(String(err));
    }
  }

  async function changeEnterKeys(value: string[]) {
    try {
      const res = await requestPost("/api/config/update_config", {
        key: "free_look_enter_keys",
        value,
      });
      dispatch(forceSetLocalConfig({ free_look_enter_keys: value }));
      messageApi?.success(res.message);
    } catch (err: unknown) {
      messageApi?.error(String(err));
    }
  }

  async function changeBlockedButtons(value: string[]) {
    try {
      const res = await requestPost("/api/config/update_config", {
        key: "free_look_blocked_mouse_buttons",
        value,
      });
      dispatch(forceSetLocalConfig({ free_look_blocked_mouse_buttons: value }));
      messageApi?.success(res.message);
    } catch (err: unknown) {
      messageApi?.error(String(err));
    }
  }

  async function changeVirtualCursor(value: boolean) {
    try {
      const res = await requestPost("/api/config/update_config", {
        key: "virtual_cursor_visible",
        value,
      });
      dispatch(forceSetLocalConfig({ virtual_cursor_visible: value }));
      messageApi?.success(res.message);
    } catch (err: unknown) {
      messageApi?.error(String(err));
    }
  }

  const mouseButtonOptions = [
    { value: "M-Left", label: t("mappings.mouseButton.left") },
    { value: "M-Middle", label: t("mappings.mouseButton.middle") },
    { value: "M-Right", label: t("mappings.mouseButton.right") },
    { value: "M-Back", label: t("mappings.mouseButton.back") },
    { value: "M-Forward", label: t("mappings.mouseButton.forward") },
  ];

  const exitKeyOptions = [
    { value: "ControlLeft", label: t("settings.freeLookExitCtrlLeft") },
    { value: "ControlRight", label: t("settings.freeLookExitCtrlRight") },
    { value: "ShiftLeft", label: t("settings.freeLookExitShiftLeft") },
    { value: "AltLeft", label: t("settings.freeLookExitAltLeft") },
    { value: "Space", label: t("settings.freeLookExitSpace") },
    { value: "Tab", label: t("settings.freeLookExitTab") },
    { value: "Escape", label: t("settings.freeLookExitEscape") },
    { value: "KeyC", label: "C" },
    { value: "KeyF", label: "F" },
    { value: "KeyG", label: "G" },
    { value: "KeyV", label: "V" },
    { value: "Backquote", label: "`" },
  ];

  return (
    <>
      <div
        className="rounded-full border-2 border-solid border-text-secondary color-text flex items-center justify-center cursor-pointer select-none"
        style={{
          position: "absolute",
          left: "25%",
          top: "50%",
          width: 64,
          height: 64,
          transform: "translate(-50%, -50%)",
          background: "rgba(0, 0, 0, 0.35)",
          zIndex: 10,
        }}
        title={t("settings.freeLookEnabled")}
        onClick={(e) => {
          e.stopPropagation();
          setInternalOpen(true);
        }}
        onContextMenu={(e) => {
          e.stopPropagation();
          e.preventDefault();
          setInternalOpen(true);
        }}
      >
        <Typography.Text className="text-2.5 font-bold">
          {t("settings.freeLookShort")}
        </Typography.Text>
      </div>

      <SettingModal open={open} onClose={close}>
        <div onClick={(e) => e.stopPropagation()}>
          <h1 className="title-with-line">{t("settings.freeLookEnabled")}</h1>
          <ItemBoxContainer className="max-h-70vh overflow-y-auto pr-2 scrollbar">
            <ItemBox
              label={t("settings.freeLookEnabled")}
              tooltip={t("settings.freeLookEnabledTip")}
            >
              <Switch
                checked={localConfig.freeLookEnabled}
                onChange={(v) => dispatch(setFreeLookEnabled(v))}
              />
            </ItemBox>
            <ItemBox label={t("settings.freeLookSensitivity")}>
              <Slider
                className="w-full"
                min={0.1}
                max={5}
                step={0.1}
                disabled={!localConfig.freeLookEnabled}
                value={localConfig.freeLookEffects?.scale_x ?? 1}
                onChange={(v) => dispatch(setFreeLookSensitivity(v))}
              />
            </ItemBox>
            <ItemBox
              label={t("mappings.common.invertX.label")}
              tooltip={t("mappings.common.invertX.hint")}
            >
              <Switch
                checked={!!localConfig.freeLookEffects?.invert_x}
                disabled={!localConfig.freeLookEnabled}
                onChange={(v) => dispatch(setFreeLookInvertX(v))}
              />
            </ItemBox>
            <ItemBox
              label={t("mappings.common.invertY.label")}
              tooltip={t("mappings.common.invertY.hint")}
            >
              <Switch
                checked={!!localConfig.freeLookEffects?.invert_y}
                disabled={!localConfig.freeLookEnabled}
                onChange={(v) => dispatch(setFreeLookInvertY(v))}
              />
            </ItemBox>
            <ItemBox
              label={t("settings.freeLookExitKey")}
              tooltip={t("settings.freeLookExitKeyTip")}
            >
              <Select
                className="w-full"
                mode="multiple"
                value={localConfig.freeLookExitKeys ?? []}
                options={exitKeyOptions}
                onChange={changeExitKeys}
              />
            </ItemBox>
            <ItemBox
              label={t("settings.freeLookEnterKeys")}
              tooltip={t("settings.freeLookEnterKeysTip")}
            >
              <Select
                className="w-full"
                mode="multiple"
                value={localConfig.freeLookEnterKeys ?? []}
                options={exitKeyOptions}
                onChange={changeEnterKeys}
              />
            </ItemBox>
            <ItemBox
              label={t("settings.freeLookBlockedButtons")}
              tooltip={t("settings.freeLookBlockedButtonsTip")}
            >
              <Select
                className="w-full"
                mode="multiple"
                value={localConfig.freeLookBlockedMouseButtons ?? []}
                options={mouseButtonOptions}
                onChange={changeBlockedButtons}
              />
            </ItemBox>
            <ItemBox
              label={t("settings.virtualCursor")}
              tooltip={t("settings.virtualCursorTip")}
            >
              <Switch
                checked={localConfig.virtualCursorVisible ?? false}
                onChange={changeVirtualCursor}
              />
            </ItemBox>
          </ItemBoxContainer>
        </div>
      </SettingModal>
    </>
  );
}
