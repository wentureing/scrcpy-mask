import { useEffect, useState } from "react";
import { Button, Collapse, Flex, Select, Switch, Typography } from "antd";
import { useTranslation } from "react-i18next";
import { useAppDispatch, useAppSelector } from "../../store/store";
import { forceSetLocalConfig } from "../../store/localConfig";
import { requestGet, requestPost } from "../../utils";
import { useMessageContext } from "../../hooks";
import { formatMappingLabel } from "../mappings/tools";

type AccessRule = {
  mapping_id: string;
  action?: string | null;
  when: string;
  allow: boolean;
};

type MappingOption = { value: string; label: string; type: string };

type AccessRulesEditorProps = {
  /** 传了 = 只显示该映射的规则（单映射模式）；不传 = 全局模式 */
  mappingId?: string;
  /** 弹窗内嵌：紧凑 + 折叠 */
  compact?: boolean;
};

/**
 * 访问规则编辑器（docs/29）。
 *
 * - 不传 `mappingId`：全局模式，显示所有规则，可改映射；
 * - 传 `mappingId`：单映射模式，只显示该映射的规则，映射固定。
 *
 * 读写仍是 `LocalConfig.access_rules` + `/api/config/update_config`，
 * 保存走「请求成功才回填」。
 */
export default function AccessRulesEditor({
  mappingId,
  compact,
}: AccessRulesEditorProps) {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const messageApi = useMessageContext();
  const localConfig = useAppSelector((state) => state.localConfig);

  const [mappingOptions, setMappingOptions] = useState<MappingOption[]>([]);
  const [accessState, setAccessState] = useState<{
    state: string;
    free_look_active: boolean;
  } | null>(null);

  // 当前映射文件里的映射（真实 id + 可读标签 + 类型）
  useEffect(() => {
    const file = localConfig.activeMappingFile;
    if (!file) {
      setMappingOptions([]);
      return;
    }
    let cancelled = false;
    requestPost<{
      mapping_config: {
        mappings?: Array<{
          id?: string;
          type?: string;
          note?: string;
          bind?: unknown;
        }>;
      };
    }>("/api/mapping/read_mapping", { file })
      .then((res) => {
        if (cancelled) return;
        const list = (res.data.mapping_config?.mappings ?? [])
          .filter((mapping) => mapping.id)
          .map((mapping) => ({
            value: mapping.id as string,
            label: formatMappingLabel(mapping),
            type: mapping.type ?? "",
          }));
        setMappingOptions(list);
      })
      .catch(() => {
        if (!cancelled) setMappingOptions([]);
      });
    return () => {
      cancelled = true;
    };
  }, [localConfig.activeMappingFile]);

  // 诊断：当前条件 + 自由视角是否激活
  useEffect(() => {
    let cancelled = false;
    requestGet<{ state: string; free_look_active: boolean }>(
      "/api/config/access_state",
    )
      .then((res) => {
        if (!cancelled) setAccessState(res.data);
      })
      .catch(() => {
        if (!cancelled) setAccessState(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const accessStateOptions = [
    {
      value: "free_look_inactive",
      label: t("settings.accessStateFreeLookInactive"),
    },
    {
      value: "free_look_active",
      label: t("settings.accessStateFreeLookActive"),
    },
    {
      value: "free_look_enabled",
      label: t("settings.accessStateFreeLookEnabledOn"),
    },
    {
      value: "free_look_disabled",
      label: t("settings.accessStateFreeLookEnabledOff"),
    },
    { value: "fps", label: t("settings.accessStateFps") },
    { value: "normal", label: t("settings.accessStateNormal") },
    { value: "observation", label: t("settings.accessStateObservation") },
    { value: "raw_input", label: t("settings.accessStateRawInput") },
  ];

  const accessActionOptions = (type: string) => {
    const whole = { value: "", label: t("settings.accessActionWhole") };
    switch (type) {
      case "SingleTap":
        return [whole, { value: "tap", label: t("settings.accessActionTap") }];
      case "RepeatTap":
        return [
          whole,
          { value: "repeat", label: t("settings.accessActionRepeat") },
        ];
      case "MultipleTap":
        return [
          whole,
          { value: "multiple", label: t("settings.accessActionMultiple") },
        ];
      case "Swipe":
        return [
          whole,
          { value: "swipe", label: t("settings.accessActionSwipe") },
        ];
      case "DirectionPad":
        return [
          whole,
          { value: "up", label: t("settings.accessActionUp") },
          { value: "down", label: t("settings.accessActionDown") },
          { value: "left", label: t("settings.accessActionLeft") },
          { value: "right", label: t("settings.accessActionRight") },
        ];
      case "Observation":
        return [whole, { value: "hold", label: t("settings.accessActionHold") }];
      case "Fire":
        return [whole, { value: "fire", label: t("settings.accessActionFire") }];
      case "MouseCastSpell":
      case "PadCastSpell":
        return [whole, { value: "cast", label: t("settings.accessActionCast") }];
      case "CancelCast":
        return [
          whole,
          { value: "cancel", label: t("settings.accessActionCancel") },
        ];
      case "Script":
        return [
          whole,
          { value: "press", label: t("settings.accessActionPress") },
        ];
      case "Fps":
        return [
          whole,
          { value: "toggle", label: t("settings.accessActionToggle") },
        ];
      default:
        return [whole];
    }
  };

  const allRules: AccessRule[] = localConfig.accessRules ?? [];
  const rules = mappingId
    ? allRules.filter((rule) => rule.mapping_id === mappingId)
    : allRules;

  async function saveAccessRules(next: AccessRule[]) {
    try {
      const res = await requestPost("/api/config/update_config", {
        key: "access_rules",
        value: next,
      });
      dispatch(forceSetLocalConfig({ access_rules: next }));
      messageApi?.success(res.message);
    } catch (err: unknown) {
      messageApi?.error(String(err));
    }
  }

  function patchRule(rule: AccessRule, patch: Partial<AccessRule>) {
    const next = allRules.map((item) => (item === rule ? { ...item, ...patch } : item));
    saveAccessRules(next);
  }

  function removeRule(rule: AccessRule) {
    saveAccessRules(allRules.filter((item) => item !== rule));
  }

  function addRule() {
    const targetId = mappingId ?? mappingOptions[0]?.value ?? "";
    saveAccessRules([
      ...allRules,
      {
        mapping_id: targetId,
        action: null,
        when: "free_look_inactive",
        allow: false,
      },
    ]);
  }

  const mappingType = (id: string) =>
    mappingOptions.find((item) => item.value === id)?.type ?? "";

  const list = (
    <Flex vertical gap="small" className="w-full">
      <Typography.Text type="secondary" className="text-xs">
        {t("settings.accessCurrentState")}
        {": "}
        {accessState?.state ?? "-"}
        {accessState?.free_look_active
          ? t("settings.accessStateSuffixActive")
          : ""}
      </Typography.Text>
      {rules.map((rule) => (
        <Flex key={`${rule.mapping_id}-${rule.action ?? ""}-${rule.when}`} gap="small" align="center" wrap>
          <Select
            className="w-sm"
            value={rule.mapping_id}
            disabled={!!mappingId}
            options={mappingOptions}
            onChange={(value) => patchRule(rule, { mapping_id: value })}
          />
          <Select
            className="w-xs"
            value={rule.action ?? ""}
            options={accessActionOptions(mappingType(rule.mapping_id))}
            onChange={(value) => patchRule(rule, { action: value || null })}
          />
          <Select
            className="w-sm"
            value={rule.when}
            options={accessStateOptions}
            onChange={(value) => patchRule(rule, { when: value })}
          />
          <Switch
            checked={rule.allow}
            checkedChildren={t("settings.accessAllow")}
            unCheckedChildren={t("settings.accessDeny")}
            onChange={(value) => patchRule(rule, { allow: value })}
          />
          <Button size="small" danger onClick={() => removeRule(rule)}>
            {t("settings.accessRuleRemove")}
          </Button>
        </Flex>
      ))}
      <Button size="small" onClick={addRule}>
        {t("settings.accessRuleAdd")}
      </Button>
    </Flex>
  );

  if (!compact) {
    return list;
  }

  return (
    <Collapse
      size="small"
      className="mt-3"
      items={[
        {
          key: "access-rules",
          label: `${t("settings.accessRulesForMapping")}（${rules.length}）`,
          children: list,
        },
      ]}
    />
  );
}
