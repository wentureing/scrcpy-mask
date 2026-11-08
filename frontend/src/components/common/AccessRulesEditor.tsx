import { useEffect, useState } from "react";
import { Button, Collapse, Flex, Select, Switch, Typography } from "antd";
import { useTranslation } from "react-i18next";
import { useAppDispatch, useAppSelector } from "../../store/store";
import { forceSetLocalConfig } from "../../store/localConfig";
import { requestGet, requestPost } from "../../utils";
import { formatMappingLabel } from "../mappings/tools";

type AccessTarget = {
  kind: "mode" | "mapping";
  mode?: string | null;
  mapping_id?: string | null;
};

type AccessRule = {
  mapping_id: string;
  action?: string | null;
  predicate: "is" | "is_not";
  condition: string;
  operation: {
    kind: "disable" | "enable" | "open" | "close";
    target?: AccessTarget | null;
  };
  enabled: boolean;
};

type MappingOption = { value: string; label: string; type: string };

type AccessRulesEditorProps = {
  /** 传了 = 只显示该映射的规则（单映射模式）；不传 = 全局模式 */
  mappingId?: string;
  /** 弹窗内嵌：紧凑 + 折叠 */
  compact?: boolean;
};

/**
 * 访问规则编辑器（docs/29、docs/31）。
 *
 * 规则 = 目标（映射 + 映射内操作）+ 条件（谓词 + 中性条件）+ 操作（禁用/启用/打开/关闭）。
 * 每行最左有“启用/禁用”开关，关闭的规则不参与判定。
 */
export default function AccessRulesEditor({
  mappingId,
  compact,
}: AccessRulesEditorProps) {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const localConfig = useAppSelector((state) => state.localConfig);

  const [mappingOptions, setMappingOptions] = useState<MappingOption[]>([]);
  const [accessState, setAccessState] = useState<{
    state: string;
    free_look_active: boolean;
  } | null>(null);

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

  const conditionOptions = [
    { value: "free_look_active", label: t("settings.accessConditionFreeLook") },
    { value: "fps", label: t("settings.accessConditionFps") },
    { value: "observation", label: t("settings.accessConditionObservation") },
    { value: "raw_input", label: t("settings.accessConditionRawInput") },
    { value: "normal", label: t("settings.accessConditionNormal") },
  ];

  const predicateOptions = [
    { value: "is", label: t("settings.accessPredicateIs") },
    { value: "is_not", label: t("settings.accessPredicateIsNot") },
  ];

  const operationOptions = [
    { value: "disable", label: t("settings.accessOperationDisable") },
    { value: "enable", label: t("settings.accessOperationEnable") },
    { value: "open", label: t("settings.accessOperationOpen") },
    { value: "close", label: t("settings.accessOperationClose") },
  ];

  const modeOptions = [
    { value: "free_look", label: t("settings.accessModeFreeLook") },
    { value: "fps", label: t("settings.accessModeFps") },
    { value: "raw_input", label: t("settings.accessModeRawInput") },
  ];

  const accessActionOptions = (type: string) => {
    const whole = { value: "", label: t("settings.accessActionWhole") };
    switch (type) {
      case "SingleTap":
        return [whole, { value: "tap", label: t("settings.accessActionTap") }];
      case "RepeatTap":
        return [whole, { value: "repeat", label: t("settings.accessActionRepeat") }];
      case "MultipleTap":
        return [whole, { value: "multiple", label: t("settings.accessActionMultiple") }];
      case "Swipe":
        return [whole, { value: "swipe", label: t("settings.accessActionSwipe") }];
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
        return [whole, { value: "cancel", label: t("settings.accessActionCancel") }];
      case "Script":
        return [whole, { value: "press", label: t("settings.accessActionPress") }];
      case "Fps":
        return [whole, { value: "toggle", label: t("settings.accessActionToggle") }];
      default:
        return [whole];
    }
  };

  const allRules: AccessRule[] = localConfig.accessRules ?? [];
  const rules = mappingId
    ? allRules.filter((rule) => rule.mapping_id === mappingId)
    : allRules;

  async function saveAccessRules(next: AccessRule[]) {
    // 只改本地草稿；落盘由映射页“保存”按钮统一提交（docs/34）
    dispatch(forceSetLocalConfig({ access_rules: next }));
  }

  function patchRule(rule: AccessRule, patch: Partial<AccessRule>) {
    saveAccessRules(
      allRules.map((item) => (item === rule ? { ...item, ...patch } : item)),
    );
  }

  function removeRule(rule: AccessRule) {
    saveAccessRules(allRules.filter((item) => item !== rule));
  }

  function addRule() {
    saveAccessRules([
      ...allRules,
      {
        mapping_id: mappingId ?? mappingOptions[0]?.value ?? "",
        action: null,
        predicate: "is",
        condition: "free_look_active",
        operation: { kind: "disable", target: null },
        enabled: true,
      },
    ]);
  }

  const mappingType = (id: string) =>
    mappingOptions.find((item) => item.value === id)?.type ?? "";

  // 只有多动作映射才显示“动作”下拉；单动作映射固定“整个映射”，避免“整个映射/点击”的歧义
  const hasMultipleActions = (type: string) =>
    type === "DirectionPad" || type === "Script";

  const header = (
    <Flex gap="small" className="w-full text-xs text-text-secondary">
      <div style={{ width: "25%" }}>{t("settings.accessColumnMapping")}</div>
      <div style={{ width: "30%" }}>{t("settings.accessColumnCondition")}</div>
      <div style={{ width: "45%" }}>{t("settings.accessColumnOperation")}</div>
    </Flex>
  );

  const list = (
    <Flex vertical gap="small" className="w-full">
      <Typography.Text type="secondary" className="text-xs">
        {t("settings.accessCurrentState")}
        {": "}
        {accessState?.state ?? "-"}
      </Typography.Text>
      {header}
      {rules.map((rule) => {
        const needsTarget =
          rule.operation.kind === "open" || rule.operation.kind === "close";
        return (
          <Flex key={`${rule.mapping_id}-${rule.action ?? ""}-${rule.condition}`} gap="small" align="center">
            <Flex gap="small" align="center" style={{ width: "25%" }}>
              <Switch
                size="small"
                checked={rule.enabled}
                onChange={(value) => patchRule(rule, { enabled: value })}
              />
              <Select
                size="small"
                className="flex-grow-1"
                value={rule.mapping_id}
                disabled={!!mappingId}
                options={mappingOptions}
                onChange={(value) => patchRule(rule, { mapping_id: value })}
              />
              {hasMultipleActions(mappingType(rule.mapping_id)) && (
                <Select
                  size="small"
                  style={{ width: 100 }}
                  value={rule.action ?? ""}
                  options={accessActionOptions(mappingType(rule.mapping_id))}
                  onChange={(value) => patchRule(rule, { action: value || null })}
                />
              )}
            </Flex>
            <Flex gap="small" align="center" style={{ width: "30%" }}>
              <Select
                size="small"
                style={{ width: 90 }}
                value={rule.predicate}
                options={predicateOptions}
                onChange={(value) =>
                  patchRule(rule, { predicate: value as AccessRule["predicate"] })
                }
              />
              <Select
                size="small"
                className="flex-grow-1"
                value={rule.condition}
                options={conditionOptions}
                onChange={(value) => patchRule(rule, { condition: value })}
              />
            </Flex>
            <Flex gap="small" align="center" style={{ width: "45%" }}>
              <Select
                size="small"
                style={{ width: 110 }}
                value={rule.operation.kind}
                options={operationOptions}
                onChange={(value) =>
                  patchRule(rule, {
                    operation: {
                      kind: value as AccessRule["operation"]["kind"],
                      target: needsTarget ? rule.operation.target ?? null : null,
                    } as AccessRule["operation"],
                  })
                }
              />
              {needsTarget && (
                <>
                  <Select
                    size="small"
                    style={{ width: 100 }}
                    value={rule.operation.target?.kind ?? "mode"}
                    options={[
                      { value: "mode", label: t("settings.accessTargetMode") },
                      { value: "mapping", label: t("settings.accessTargetMapping") },
                    ]}
                    onChange={(value) =>
                      patchRule(rule, {
                        operation: {
                          ...rule.operation,
                          target: {
                            kind: value as "mode" | "mapping",
                            mode: value === "mode" ? "free_look" : null,
                            mapping_id:
                              value === "mapping"
                                ? mappingOptions[0]?.value ?? null
                                : null,
                          },
                        },
                      })
                    }
                  />
                  <Select
                    size="small"
                    className="flex-grow-1"
                    value={
                      rule.operation.target?.kind === "mode"
                        ? rule.operation.target?.mode ?? "free_look"
                        : rule.operation.target?.mapping_id ?? ""
                    }
                    options={
                      rule.operation.target?.kind === "mode"
                        ? modeOptions
                        : mappingOptions
                    }
                    onChange={(value) =>
                      patchRule(rule, {
                        operation: {
                          ...rule.operation,
                          target: {
                            kind: rule.operation.target?.kind ?? "mode",
                            mode:
                              rule.operation.target?.kind === "mode"
                                ? value
                                : null,
                            mapping_id:
                              rule.operation.target?.kind === "mapping"
                                ? value
                                : null,
                          },
                        },
                      })
                    }
                  />
                </>
              )}
              <Button size="small" danger onClick={() => removeRule(rule)}>
                {t("settings.accessRuleRemove")}
              </Button>
            </Flex>
          </Flex>
        );
      })}
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
