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
export default function FreeLookEntry() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const messageApi = useMessageContext();
  const localConfig = useAppSelector((state) => state.localConfig);
  const [open, setOpen] = useState(false);

  // 退出键冲突时后端会拒绝保存（D10），所以成功后才回填 store。
  async function changeExitKey(value: string) {
    try {
      const res = await requestPost("/api/config/update_config", {
        key: "free_look_exit_key",
        value,
      });
      dispatch(forceSetLocalConfig({ free_look_exit_key: value }));
      messageApi?.success(res.message);
    } catch (err: unknown) {
      messageApi?.error(String(err));
    }
  }

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
          setOpen(true);
        }}
        onContextMenu={(e) => {
          e.stopPropagation();
          e.preventDefault();
          setOpen(true);
        }}
      >
        <Typography.Text className="text-2.5 font-bold">
          {t("settings.freeLookShort")}
        </Typography.Text>
      </div>

      <SettingModal open={open} onClose={() => setOpen(false)}>
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
                value={localConfig.freeLookExitKey ?? "ControlLeft"}
                options={exitKeyOptions}
                onChange={changeExitKey}
              />
            </ItemBox>
          </ItemBoxContainer>
        </div>
      </SettingModal>
    </>
  );
}
