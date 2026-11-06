import { useTranslation } from "react-i18next";
import { useAppDispatch, useAppSelector } from "../store/store";
import { ItemBox, ItemBoxContainer } from "./common/ItemBox";
import {
  AutoComplete,
  Badge,
  Button,
  Collapse,
  Flex,
  Input,
  InputNumber,
  Select,
  Slider,
  Space,
  Switch,
  Typography,
} from "antd";
import {
  forceSetLocalConfig,
  setAdbPath,
  setAdbCleanupPolicy,
  setControllerPort,
  setVerticalPosition,
  setverticalMaskHeight,
  setWebPort,
  sethorizontalMaskWidth,
  setHorizontalPosition,
  setMappingLabelOpacity,
  setFreeLookEnabled,
  setFreeLookSensitivity,
  setFreeLookInvertX,
  setFreeLookInvertY,
  setClipboardSync,
  setLanguage,
  setVideoCodec,
  setVideoBitRate,
  setVideoMaxSize,
  setVideoMaxFps,
  setDisplayId,
  setNewDisplayEnabled,
  setNewDisplayUseMainSize,
  setNewDisplayWidth,
  setNewDisplayHeight,
  setNewDisplayDpi,
  setAlwaysOnTop,
  setTitlebarVisible,
  setWebBindAddr,
  setAudioCodec,
  setAudioBitRate,
  setAudioSource,
  setAudioDup,
  setStayAwake,
  setScreenOffTimeout,
  setPowerOffOnClose,
} from "../store/localConfig";
import {
  setIsLoading,
  setShowUpdateDialog,
  setUpdateInfo,
} from "../store/other";
import { requestGet, requestPost } from "../utils";
import { useEffect, useState, type ComponentProps } from "react";
import i18n, { languageOptions } from "../i18n";
import { useMessageContext } from "../hooks";
import {
  BilibiliFilled,
  CloudSyncOutlined,
  GithubFilled,
  InfoCircleOutlined,
  SyncOutlined,
} from "@ant-design/icons";

/**
 * 设置页专用：把 tooltip 同时作为右侧"等高描述"（用例 12）。
 * 鼠标悬停整行时右侧出现说明，避免只能靠问号图标才能看到。
 */
function SettingBox(props: ComponentProps<typeof ItemBox>) {
  return <ItemBox {...props} sideHint={props.tooltip} />;
}

const videoCodecOptions = ["H264", "H265", "AV1"].map((v) => ({
  value: v,
  label: v,
}));

const audioCodecOptions = ["OPUS", "AAC", "FLAC", "RAW"].map((v) => ({
  value: v,
  label: v,
}));

const audioSourceOptions = ["OUTPUT", "PLAYBACK", "MIC"].map((v) => ({
  value: v,
  label: v,
}));

const audioBitRateMin = 16000;

const webBindAddrOptions = [
  { value: "127.0.0.1", label: "127.0.0.1" },
  { value: "0.0.0.0", label: "0.0.0.0" },
];

export default function Settings() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const messageApi = useMessageContext();
  const localConfig = useAppSelector((state) => state.localConfig);
  const updateInfo = useAppSelector((state) => state.other.updateInfo);
  const [touchSaving, setTouchSaving] = useState(false);

  // free look 退出键候选（后端 exit_keycode 只识别这些名称，见 free_look.rs）
  const freeLookExitKeyOptions = [
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

  // 端口被占用时后端会静默顺延到下一个端口，这里把那条提示显示出来（读后即清空）。
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await requestGet("/api/config/port_notice");
        if (!cancelled && res.data?.notice) {
          messageApi?.info(res.data.notice);
        }
      } catch {
        // 提示是附加信息，取不到不打扰用户
      }
    })();
    return () => {
      cancelled = true;
    };
    // 提示是 take 型的：重复请求第二次返回空，不会重复打扰。
  }, [messageApi]);

  // Unlike optimistic/debounced settings, don't show a backend as active if
  // the server rejected a switch while devices are connected.
  async function changeTouchBackend(value: "sdk" | "uhid") {
    setTouchSaving(true);
    try {
      const res = await requestPost("/api/config/update_config", {
        key: "touch_backend", value,
      });
      dispatch(forceSetLocalConfig({ touch_backend: value }));
      messageApi?.success(res.message);
    } catch (err: unknown) {
      messageApi?.error(String(err));
    } finally {
      setTouchSaving(false);
    }
  }

  // 退出键冲突时后端会拒绝保存（D10），所以这里不走乐观更新：
  // 先请求，成功后再回填 store，失败则保留旧值并提示。
  async function changeFreeLookExitKey(value: string) {
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

  async function loadLocalConfig() {
    dispatch(setIsLoading(true));
    try {
      const res = await requestGet("/api/config/get_config");
      dispatch(forceSetLocalConfig(res.data));
      i18n.changeLanguage(res.data.language);
    } catch (err: any) {
      messageApi?.error(err);
    }
    dispatch(setIsLoading(false));
  }

  async function openDataPath() {    dispatch(setIsLoading(true));
    try {
      const res = await requestGet("/api/config/open_data_path");
      messageApi?.success(res.message);
    } catch (err: any) {
      messageApi?.error(err);
    }
    dispatch(setIsLoading(false));
  }

  async function checkUpdate() {
    try {
      const res = await requestGet("/api/config/check_update");
      dispatch(
        setUpdateInfo({
          currentVersion: res.data.current_version,
          hasUpdate: res.data.has_update,
          latestVersion: res.data.latest_version,
          title: res.data.title,
          body: res.data.body,
          time: res.data.time,
        })
      );
      if (res.data.has_update) {
        dispatch(setShowUpdateDialog(true));
      }
    } catch (err: any) {
      messageApi?.error(err);
    }
  }

  return (
    <div className="page-container">
      <section>
        <Flex align="start" justify="space-between">
          <h2 className="title-with-line" style={{ marginBottom: 0 }}>
            {t("settings.title.header")}
          </h2>
          <Button
            type="primary"
            icon={<SyncOutlined />}
            shape="circle"
            onClick={loadLocalConfig}
          />
        </Flex>
        <h3 className="title-with-line-sub">{t("settings.title.basic")}</h3>
        <ItemBoxContainer className="mb-6">
          <SettingBox label={t("settings.language")}>
            <Select
              className="w-sm"
              value={localConfig.language}
              options={languageOptions}
              onChange={(v) => dispatch(setLanguage(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.adbPath")}>
            <Input
              className="w-sm"
              value={localConfig.adbPath}
              onChange={(e) => dispatch(setAdbPath(e.target.value))}
            />
          </SettingBox>
          <SettingBox label={t("settings.touchBackend")} extra={t("settings.touchBackendHint")}>
            <Select
              className="w-sm"
              value={localConfig.touchBackend}
              loading={touchSaving}
              disabled={touchSaving}
              options={[
                { value: "sdk", label: t("settings.touchSdk") },
                { value: "uhid", label: t("settings.touchUhid") },
              ]}
              onChange={changeTouchBackend}
            />
          </SettingBox>
          <SettingBox label={t("settings.clipboardSync")}>
            <Switch
              checked={localConfig.clipboardSync}
              onChange={(v) => dispatch(setClipboardSync(v))}
            />
          </SettingBox>
        </ItemBoxContainer>
        <h3 className="title-with-line-sub">{t("settings.title.mask")}</h3>
        <ItemBoxContainer className="mb-6">
          <SettingBox label={t("settings.alwaysOnTop")}>
            <Switch
              checked={localConfig.alwaysOnTop}
              onChange={(v) => dispatch(setAlwaysOnTop(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.titlebarVisible")}>
            <Switch
              checked={localConfig.titlebarVisible}
              onChange={(v) => dispatch(setTitlebarVisible(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.mappingLabelOpacity")}>
            <Slider
              className="w-sm"
              min={0}
              max={1}
              step={0.01}
              onChange={(v) => dispatch(setMappingLabelOpacity(v))}
              value={localConfig.mappingLabelOpacity}
            />
          </SettingBox>
          <SettingBox
            label={t("settings.freeLookEnabled")}
            tooltip={t("settings.freeLookEnabledTip")}
          >
            <Switch
              checked={localConfig.freeLookEnabled}
              onChange={(v) => dispatch(setFreeLookEnabled(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.freeLookSensitivity")}>
            <Slider
              className="w-sm"
              min={0.1}
              max={5}
              step={0.1}
              disabled={!localConfig.freeLookEnabled}
              onChange={(v) => dispatch(setFreeLookSensitivity(v))}
              value={localConfig.freeLookEffects?.scale_x ?? 1}
            />
          </SettingBox>
          <SettingBox
            label={t("mappings.common.invertX.label")}
            tooltip={t("mappings.common.invertX.hint")}
          >
            <Switch
              checked={!!localConfig.freeLookEffects?.invert_x}
              disabled={!localConfig.freeLookEnabled}
              onChange={(v) => dispatch(setFreeLookInvertX(v))}
            />
          </SettingBox>
          <SettingBox
            label={t("mappings.common.invertY.label")}
            tooltip={t("mappings.common.invertY.hint")}
          >
            <Switch
              checked={!!localConfig.freeLookEffects?.invert_y}
              disabled={!localConfig.freeLookEnabled}
              onChange={(v) => dispatch(setFreeLookInvertY(v))}
            />
          </SettingBox>
          <SettingBox
            label={t("settings.freeLookExitKey")}
            tooltip={t("settings.freeLookExitKeyTip")}
          >
            <Select
              className="w-sm"
              value={localConfig.freeLookExitKey ?? "ControlLeft"}
              options={freeLookExitKeyOptions}
              onChange={changeFreeLookExitKey}
            />
          </SettingBox>
          <SettingBox label={t("settings.horizontalMaskWidth")}>
            <InputNumber
              className="w-sm"
              controls={false}
              min={50}
              value={localConfig.horizontalMaskWidth}
              onChange={(v) =>
                v !== null && dispatch(sethorizontalMaskWidth(v))
              }
            />
          </SettingBox>
          <SettingBox label={t("settings.horizontalMaskPosition")}>
            <Space.Compact className="w-sm">
              <InputNumber
                prefix="X:"
                className="w-50%"
                controls={false}
                value={localConfig.horizontalPosition[0]}
                onChange={(v) =>
                  v !== null &&
                  dispatch(
                    setHorizontalPosition([
                      v,
                      localConfig.horizontalPosition[1],
                    ])
                  )
                }
              />
              <InputNumber
                prefix="Y:"
                className="w-50%"
                controls={false}
                value={localConfig.horizontalPosition[1]}
                onChange={(v) =>
                  v !== null &&
                  dispatch(
                    setHorizontalPosition([
                      localConfig.horizontalPosition[0],
                      v,
                    ])
                  )
                }
              />
            </Space.Compact>
          </SettingBox>
          <SettingBox label={t("settings.verticalMaskHeight")}>
            <InputNumber
              className="w-sm"
              controls={false}
              min={50}
              value={localConfig.verticalMaskHeight}
              onChange={(v) => v !== null && dispatch(setverticalMaskHeight(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.verticalMaskPosition")}>
            <Space.Compact className="w-sm">
              <InputNumber
                prefix="X:"
                className="w-50%"
                controls={false}
                value={localConfig.verticalPosition[0]}
                onChange={(v) =>
                  v !== null &&
                  dispatch(
                    setVerticalPosition([v, localConfig.verticalPosition[1]])
                  )
                }
              />
              <InputNumber
                prefix="Y:"
                className="w-50%"
                controls={false}
                value={localConfig.verticalPosition[1]}
                onChange={(v) =>
                  v !== null &&
                  dispatch(
                    setVerticalPosition([localConfig.verticalPosition[0], v])
                  )
                }
              />
            </Space.Compact>
          </SettingBox>
        </ItemBoxContainer>
        <h3 className="title-with-line-sub">{t("settings.title.video")}</h3>
        <ItemBoxContainer className="mb-6">
          <SettingBox label={t("settings.videoCodec")}>
            <Select
              className="w-sm"
              value={localConfig.videoCodec}
              options={videoCodecOptions}
              onChange={(v) => dispatch(setVideoCodec(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.videoBitRate")}>
            <InputNumber
              className="w-sm"
              controls={false}
              min={1000000}
              suffix="bps"
              value={localConfig.videoBitRate}
              onChange={(v) => v !== null && dispatch(setVideoBitRate(v))}
            />
          </SettingBox>
          <SettingBox
            label={t("settings.videoMaxSize")}
            tooltip={t("settings.zeroUnlimitedTip")}
          >
            <InputNumber
              className="w-sm"
              controls={false}
              min={0}
              value={localConfig.videoMaxSize}
              onChange={(v) => v !== null && dispatch(setVideoMaxSize(v))}
            />
          </SettingBox>
          <SettingBox
            label={t("settings.videoMaxFps")}
            tooltip={t("settings.zeroUnlimitedTip")}
          >
            <InputNumber
              className="w-sm"
              controls={false}
              min={0}
              value={localConfig.videoMaxFps}
              onChange={(v) => v !== null && dispatch(setVideoMaxFps(v))}
            />
          </SettingBox>
          <SettingBox
            label={t("settings.displayId")}
            tooltip={t("settings.displayIdTip")}
          >
            <InputNumber
              className="w-sm"
              controls={false}
              disabled={localConfig.newDisplayEnabled}
              value={localConfig.displayId}
              onChange={(v) => v !== null && dispatch(setDisplayId(v))}
            />
          </SettingBox>
          <SettingBox
            label={t("settings.newDisplayEnabled")}
            tooltip={t("settings.newDisplayEnabledTip")}
          >
            <Switch
              checked={localConfig.newDisplayEnabled}
              onChange={(v) => dispatch(setNewDisplayEnabled(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.newDisplayUseMainSize")}>
            <Switch
              checked={localConfig.newDisplayUseMainSize}
              disabled={!localConfig.newDisplayEnabled}
              onChange={(v) => dispatch(setNewDisplayUseMainSize(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.newDisplaySize")}>
            <Space.Compact className="w-sm">
              <InputNumber
                prefix="W:"
                className="w-50%"
                controls={false}
                min={1}
                disabled={
                  !localConfig.newDisplayEnabled ||
                  localConfig.newDisplayUseMainSize
                }
                value={localConfig.newDisplayWidth}
                onChange={(v) =>
                  v !== null && dispatch(setNewDisplayWidth(v))
                }
              />
              <InputNumber
                prefix="H:"
                className="w-50%"
                controls={false}
                min={1}
                disabled={
                  !localConfig.newDisplayEnabled ||
                  localConfig.newDisplayUseMainSize
                }
                value={localConfig.newDisplayHeight}
                onChange={(v) =>
                  v !== null && dispatch(setNewDisplayHeight(v))
                }
              />
            </Space.Compact>
          </SettingBox>
          <SettingBox label={t("settings.newDisplayDpi")}>
            <InputNumber
              className="w-sm"
              controls={false}
              min={1}
              disabled={
                !localConfig.newDisplayEnabled ||
                localConfig.newDisplayUseMainSize
              }
              value={localConfig.newDisplayDpi}
              onChange={(v) => v !== null && dispatch(setNewDisplayDpi(v))}
            />
          </SettingBox>
        </ItemBoxContainer>

        <h3 className="title-with-line-sub">{t("settings.title.audio")}</h3>
        <ItemBoxContainer className="mb-6">
          <SettingBox label={t("settings.audioCodec")}>
            <Select
              className="w-sm"
              value={localConfig.audioCodec}
              options={audioCodecOptions}
              onChange={(v) => dispatch(setAudioCodec(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.audioBitRate")}>
            <InputNumber
              className="w-sm"
              controls={false}
              disabled={localConfig.audioCodec === "RAW"}
              min={audioBitRateMin}
              suffix="bps"
              value={localConfig.audioBitRate}
              onChange={(v) => v !== null && dispatch(setAudioBitRate(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.audioSource")}>
            <Select
              className="w-sm"
              value={localConfig.audioSource}
              options={audioSourceOptions}
              onChange={(v) => dispatch(setAudioSource(v))}
            />
          </SettingBox>
          <SettingBox
            label={t("settings.audioDup")}
            tooltip={t("settings.audioDupTip")}
          >
            <Switch
              checked={localConfig.audioSource === "PLAYBACK" && localConfig.audioDup}
              disabled={localConfig.audioSource !== "PLAYBACK"}
              onChange={(v) => dispatch(setAudioDup(v))}
            />
          </SettingBox>
        </ItemBoxContainer>

        {/* 用例 12：高级设置折叠起来，避免首屏全是低频选项 */}
        <Collapse
          className="mb-6"
          items={[
            {
              key: "advance",
              label: t("settings.title.advance"),
              children: (
                <ItemBoxContainer>
          <SettingBox label={t("settings.stayAwake")}>
            <Switch
              checked={localConfig.stayAwake}
              onChange={(v) => dispatch(setStayAwake(v))}
            />
          </SettingBox>
          <SettingBox
            label={t("settings.adbCleanupPolicy")}
            tooltip={t("settings.adbCleanupPolicyTip")}
          >
            <Select
              className="w-sm"
              value={localConfig.adbCleanupPolicy}
              options={[
                { value: "auto", label: t("settings.adbCleanupAuto") },
                { value: "always", label: t("settings.adbCleanupAlways") },
                { value: "never", label: t("settings.adbCleanupNever") },
              ]}
              onChange={(v) =>
                dispatch(setAdbCleanupPolicy(v as "auto" | "always" | "never"))
              }
            />
          </SettingBox>
          <SettingBox
            label={t("settings.screenOffTimeout")}
            tooltip={t("settings.screenOffTimeoutTip")}
          >
            <InputNumber
              className="w-sm"
              controls={false}
              min={-1}
              suffix="ms"
              value={localConfig.screenOffTimeout}
              onChange={(v) => v !== null && dispatch(setScreenOffTimeout(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.powerOffOnClose")}>
            <Switch
              checked={localConfig.powerOffOnClose}
              onChange={(v) => dispatch(setPowerOffOnClose(v))}
            />
          </SettingBox>
          <SettingBox
            label={t("settings.webBindAddr")}
            tooltip={t("settings.webBindAddrTip")}
          >
            <AutoComplete
              className="w-sm"
              options={webBindAddrOptions}
              value={localConfig.webBindAddr}
              onChange={(v) => dispatch(setWebBindAddr(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.webPort")}>
            <InputNumber
              className="w-sm"
              controls={false}
              value={localConfig.webPort}
              onChange={(v) => v !== null && dispatch(setWebPort(v))}
            />
          </SettingBox>
          <SettingBox label={t("settings.controllerPort")}>
            <InputNumber
              className="w-sm"
              controls={false}
              value={localConfig.controllerPort}
              onChange={(v) => v !== null && dispatch(setControllerPort(v))}
            />
          </SettingBox>
          <SettingBox>
            <Button type="primary" onClick={openDataPath}>
              {t("settings.openDataPath")}
            </Button>
          </SettingBox>
                </ItemBoxContainer>
              ),
            },
          ]}
        />
      </section>
      <section>
        <h2 className="title-with-line">{t("settings.about.title")}</h2>
        <Typography.Paragraph>{t("settings.about.intro")}</Typography.Paragraph>
        <Flex gap="large">
          <Button
            type="text"
            icon={<GithubFilled />}
            onClick={() =>
              window.open("https://github.com/AkiChase/scrcpy-mask", "_blank")
            }
          >
            Github
          </Button>
          <Button
            type="text"
            icon={<BilibiliFilled />}
            onClick={() =>
              window.open("https://space.bilibili.com/440760180", "_blank")
            }
          >
            BiliBili
          </Button>
        </Flex>
        <Flex gap="large" align="center" className="mt-4">
          <Button
            type="primary"
            icon={<CloudSyncOutlined />}
            onClick={checkUpdate}
          >
            {t("settings.about.checkUpdate")}
          </Button>
          <Badge dot={updateInfo.hasUpdate}>
            <Button
              type="primary"
              icon={<InfoCircleOutlined />}
              onClick={() => dispatch(setShowUpdateDialog(true))}
            >
              {t("settings.about.showUpdateDialog")}
            </Button>
          </Badge>
        </Flex>
        <Flex gap="large" align="center" className="mt-4">
          <Typography.Text>
            {t("settings.about.currentVersion")}: {updateInfo.currentVersion}
          </Typography.Text>
          <Typography.Text>
            {t("settings.about.latestVersion")}: {updateInfo.latestVersion}
          </Typography.Text>
        </Flex>
      </section>
    </div>
  );
}
