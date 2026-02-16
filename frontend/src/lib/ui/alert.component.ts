import { Component, Input } from '@angular/core';
import { cn } from '../utils';
import { cva, type VariantProps } from 'class-variance-authority';

const alertVariants = cva(
  "relative w-full rounded-lg border px-4 py-3 text-sm grid has-[>svg]:grid-cols-[calc(var(--spacing)*4)_1fr] grid-cols-[0_1fr] has-[>svg]:gap-x-3 gap-y-0.5 items-start [&>svg]:size-4 [&>svg]:translate-y-0.5 [&>svg]:text-current",
  {
    variants: {
      variant: {
        default: "bg-card text-card-foreground",
        destructive: "text-destructive bg-card [&>svg]:text-current *:data-[slot=alert-description]:text-destructive/90",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
);

@Component({
  selector: 'app-alert',
  standalone: true,
  template: `<div 
    [class]="cn(alertVariants({ variant }), className)"
    [attr.data-slot]="'alert'"
    [attr.role]="'alert'">
    <ng-content></ng-content>
  </div>`,
})
export class AlertComponent {
  @Input() className = '';
  @Input() variant: 'default' | 'destructive' = 'default';
  
  protected cn = cn;
  protected alertVariants = alertVariants;
}

@Component({
  selector: 'app-alert-title',
  standalone: true,
  template: `<div 
    [class]="cn('col-start-2 line-clamp-1 min-h-4 font-medium tracking-tight', className)"
    [attr.data-slot]="'alert-title'">
    <ng-content></ng-content>
  </div>`,
})
export class AlertTitleComponent {
  @Input() className = '';
  
  protected cn = cn;
}

@Component({
  selector: 'app-alert-description',
  standalone: true,
  template: `<div 
    [class]="cn('text-muted-foreground col-start-2 grid justify-items-start gap-1 text-sm [&_p]:leading-relaxed', className)"
    [attr.data-slot]="'alert-description'">
    <ng-content></ng-content>
  </div>`,
})
export class AlertDescriptionComponent {
  @Input() className = '';
  
  protected cn = cn;
}