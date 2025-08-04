
interface CircleProps {
  color: 'success' | 'danger' | 'warning' | 'info' | 'primary' | 'secondary';
}

export function Circle({ color }: CircleProps) {
  return (
    <div
      data-testid="circle"
      className={`circle circle-${color}`}
      style={{
        width: '12px',
        height: '12px',
        borderRadius: '50%',
        backgroundColor: color === 'success' ? 'green' : color === 'danger' ? 'red' : 'gray'
      }}
    />
  );
}
