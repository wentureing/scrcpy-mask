import { createSlice, type PayloadAction } from "@reduxjs/toolkit";
import type {
  AdbDevice,
  ControlledDevice,
  DeviceStatus,
  MergedDevice,
} from "../utils";

/** adb 状态字符串归一化，避免前端猜取值 */
export function normalizeDeviceStatus(status: string): DeviceStatus {
  const s = status.toLowerCase();
  if (s.includes("unauthorized")) return "unauthorized";
  if (s.includes("offline")) return "offline";
  if (s.includes("recovery")) return "recovery";
  return "device";
}

/** 由"受控列表 + adb 列表"重建单表数据，受控项优先 */
export function mergeDevices(
  controlled: ControlledDevice[],
  adb: AdbDevice[]
): MergedDevice[] {
  const controlledIds = new Set(controlled.map((d) => d.device_id));
  return [
    ...controlled.map((d) => ({
      device_id: d.device_id,
      name: d.name,
      status: "controlled" as const,
      main: d.main,
      scid: d.scid,
      socket_ids: d.socket_ids,
      device_size: d.device_size,
      transport: (d.device_id.includes(":") ? "tcp" : "usb") as
        | "tcp"
        | "usb",
    })),
    ...adb
      .filter((d) => !controlledIds.has(d.id))
      .map((d) => ({
        device_id: d.id,
        name: null,
        status: normalizeDeviceStatus(d.status),
        main: false,
        scid: null,
        socket_ids: [] as string[],
        device_size: null,
        transport: (d.id.includes(":") ? "tcp" : "usb") as "tcp" | "usb",
      })),
  ];
}

export interface DeviceRotation {
  rotation: number;
  width: number;
  height: number;
}

export interface OtherState {
  isLoading: boolean;
  maskArea: {
    width: number;
    height: number;
    left: number;
    top: number;
  };
  backgroundImage: string;
  controlledDevices: ControlledDevice[];
  adbDevices: AdbDevice[];
  /** 单表数据源：受控 + 未受控合并（由上面两个列表派生，见 mergeDevices） */
  devices: MergedDevice[];
  deviceRotations: Record<string, DeviceRotation>;
  updateInfo: {
    hasUpdate: boolean;
    currentVersion: string;
    latestVersion: string;
    title: string;
    body: string;
    time: string;
  };
  showUpdateDialog: boolean;
}

const initialState: OtherState = {
  isLoading: false,
  maskArea: {
    width: 1,
    height: 1,
    left: 0,
    top: 0,
  },
  backgroundImage: "",
  controlledDevices: [],
  adbDevices: [],
  devices: [],
  deviceRotations: {},
  updateInfo: {
    hasUpdate: false,
    currentVersion: "Unknown",
    latestVersion: "Unknown",
    title: "Unknown",
    body: "Unknown",
    time: "",
  },
  showUpdateDialog: false,
};

const otherSlice = createSlice({
  name: "other",
  initialState,
  reducers: {
    setIsLoading: (state, action: PayloadAction<OtherState["isLoading"]>) => {
      state.isLoading = action.payload;
    },
    setMaskArea: (state, action: PayloadAction<OtherState["maskArea"]>) => {
      state.maskArea = action.payload;
    },
    setBackgroundImage: (
      state,
      action: PayloadAction<OtherState["backgroundImage"]>
    ) => {
      if (state.backgroundImage) {
        URL.revokeObjectURL(state.backgroundImage);
      }
      state.backgroundImage = action.payload;
    },
    setControlledDevices: (
      state,
      action: PayloadAction<OtherState["controlledDevices"]>
    ) => {
      state.controlledDevices = action.payload;
      state.devices = mergeDevices(action.payload, state.adbDevices);
    },
    setAdbDevices: (state, action: PayloadAction<OtherState["adbDevices"]>) => {
      state.adbDevices = action.payload;
      state.devices = mergeDevices(state.controlledDevices, action.payload);
    },
    // 后端 device_list 已返回合并数组时，直接用它（单一真相）
    setDevices: (state, action: PayloadAction<OtherState["devices"]>) => {
      state.devices = action.payload;
    },
    setUpdateInfo: (state, action: PayloadAction<OtherState["updateInfo"]>) => {
      state.updateInfo = action.payload;
    },
    setShowUpdateDialog: (
      state,
      action: PayloadAction<OtherState["showUpdateDialog"]>
    ) => {
      state.showUpdateDialog = action.payload;
    },
    setDeviceRotation: (
      state,
      action: PayloadAction<{ scid: string } & DeviceRotation>
    ) => {
      const { scid, rotation, width, height } = action.payload;
      state.deviceRotations[scid] = { rotation, width, height };
      const device = state.controlledDevices.find((d) => d.scid === scid);
      if (device) {
        device.device_size = [width, height];
      }
    },
  },
});

export const {
  setIsLoading,
  setMaskArea,
  setBackgroundImage,
  setControlledDevices,
  setAdbDevices,
  setDeviceRotation,
  setUpdateInfo,
  setShowUpdateDialog,
} = otherSlice.actions;

export default otherSlice.reducer;
