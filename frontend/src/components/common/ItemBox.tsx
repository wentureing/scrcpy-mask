import { QuestionCircleOutlined } from "@ant-design/icons";
import { Flex, Tooltip } from "antd";
import type { ComponentProps, PropsWithChildren, ReactNode } from "react";

type ItemBoxContainerProps = PropsWithChildren<{
  gap?: number;
}> &
  ComponentProps<"div">;

export function ItemBoxContainer({
  children,
  gap,
  ...rest
}: ItemBoxContainerProps) {
  gap = gap ?? 24;

  return (
    <Flex {...rest} vertical gap={gap}>
      {children}
    </Flex>
  );
}

type ItemBoxProps = PropsWithChildren<{
  label?: ReactNode;
  tooltip?: ReactNode;
  extra?: ReactNode;
  /**
   * 右侧等高说明（鼠标悬停整行时显示）。
   * 只有设置页使用；映射弹窗不传，保持原有紧凑布局。
   */
  sideHint?: ReactNode;
}> &
  ComponentProps<"div">;

export function ItemBox({
  label,
  tooltip,
  extra,
  sideHint,
  children,
  className,
  ...rest
}: ItemBoxProps) {
  const content = (
    <>
      {label && (
        <Flex
          align="center"
          gap="small"
          className="color-text font-bold pb-2 pl-1 pr-1 w-full"
        >
          <div>{label}</div>
          {tooltip && (
            <Tooltip title={tooltip}>
              <QuestionCircleOutlined className="cursor-help" />
            </Tooltip>
          )}
          <div className="flex-1" />
          {extra}
        </Flex>
      )}
      <div>{children}</div>
    </>
  );

  // 无 sideHint：保持旧布局（映射设置弹窗用，避免布局变化）
  if (!sideHint) {
    return (
      <div {...rest} className={className}>
        {content}
      </div>
    );
  }

  // 有 sideHint：左侧内容 + 右侧等高等宽描述区（悬停显示）
  return (
    <div
      {...rest}
      className={`group ${className ?? ""}`}
      style={{ ...rest.style }}
    >
      <Flex gap="middle" align="stretch">
        <div className="flex-1 min-w-0">{content}</div>
        <div
          className="shrink-0 self-stretch flex items-center text-xs color-text-secondary opacity-0 transition-opacity duration-150 group-hover:opacity-100"
          style={{ width: 180 }}
        >
          {sideHint}
        </div>
      </Flex>
    </div>
  );
}
