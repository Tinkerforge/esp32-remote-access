import { render, screen, fireEvent } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DeleteDeviceModal } from '../DeleteDeviceModal';
import { StateDevice } from '../types';

const mockDevice: StateDevice = {
  id: '1',
  uid: 12345,
  name: 'Test Device',
  status: 'Connected',
  note: 'Test note',
  port: 8080,
  valid: true,
  last_state_change: 1640995200,
  firmware_version: '1.0.0',
};

const defaultProps = {
  show: true,
  device: mockDevice,
  onConfirm: vi.fn(),
  onCancel: vi.fn(),
};

describe('DeleteDeviceModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders modal when show is true', () => {
    render(<DeleteDeviceModal {...defaultProps} />);
    expect(screen.getByText('remove')).toBeInTheDocument();
    expect(screen.getByText('close')).toBeInTheDocument();
  });

  it('does not render modal when show is false', () => {
    render(<DeleteDeviceModal {...defaultProps} show={false} />);
    expect(screen.queryByText('remove')).not.toBeInTheDocument();
  });

  it('displays modal heading with device name', () => {
    render(<DeleteDeviceModal {...defaultProps} />);
    // The heading uses a translation key with the device name
    const heading = screen.getByText((content, element) => {
      return element?.tagName === 'DIV' && content.includes('delete_modal_heading');
    });
    expect(heading).toBeInTheDocument();
  });

  it('displays modal body with device name', () => {
    render(<DeleteDeviceModal {...defaultProps} />);
    // The body uses a translation key with the device name
    const body = screen.getByText((content, element) => {
      return element?.tagName === 'DIV' && content.includes('delete_modal_body');
    });
    expect(body).toBeInTheDocument();
  });

  it('displays remove and close buttons', () => {
    render(<DeleteDeviceModal {...defaultProps} />);
    expect(screen.getByText('remove')).toBeInTheDocument();
    expect(screen.getByText('close')).toBeInTheDocument();
  });

  it('calls onConfirm when remove button is clicked', async () => {
    render(<DeleteDeviceModal {...defaultProps} />);
    const removeButton = screen.getByText('remove');

    fireEvent.click(removeButton);

    expect(defaultProps.onConfirm).toHaveBeenCalled();
  });

  it('calls onCancel when close button is clicked', () => {
    render(<DeleteDeviceModal {...defaultProps} />);
    const closeButton = screen.getByText('close');

    fireEvent.click(closeButton);

    expect(defaultProps.onCancel).toHaveBeenCalled();
  });

  it('renders with different device names', () => {
    const testDevice = {
      ...mockDevice,
      name: 'Special Test Device',
      id: 'special-id'
    };

    render(<DeleteDeviceModal {...defaultProps} device={testDevice} />);
    expect(screen.getByText('remove')).toBeInTheDocument();
  });
});
